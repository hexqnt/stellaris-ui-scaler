use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::{Result, ensure};
use image::{GenericImageView, RgbaImage, imageops};
use rayon::prelude::*;

use super::descriptor::{Descriptor, Query, Region};
use crate::{
    compiler::{BuildPlan, Control},
    config::Scale,
    textures,
};

struct Entry {
    asset: usize,
    frame: u32,
    region: Region,
    descriptor: Descriptor,
}

pub(super) struct Index {
    entries: Vec<Entry>,
    pub skipped: usize,
}

#[derive(Clone, Copy)]
pub(super) struct Hit {
    pub asset: usize,
    pub frame: u32,
    pub region: Region,
    pub distance: f32,
}

impl Index {
    pub fn build(plan: &BuildPlan, control: &Control, completed: &AtomicUsize) -> Result<Self> {
        let skipped = AtomicUsize::new(0);
        let chunks = plan
            .textures
            .par_iter()
            .enumerate()
            .map(|(asset, _)| {
                control.check()?;
                let result = Entry::read(plan, asset, control);
                completed.fetch_add(1, Ordering::Relaxed);
                if let Ok(entries) = result {
                    Ok(entries)
                } else {
                    control.check()?;
                    skipped.fetch_add(1, Ordering::Relaxed);
                    Ok(Vec::new())
                }
            })
            .collect::<Result<Vec<_>>>()?;
        control.check()?;
        Ok(Self {
            entries: chunks.into_iter().flatten().collect(),
            skipped: skipped.load(Ordering::Relaxed),
        })
    }

    pub fn search(
        &self,
        query: &Query,
        control: &Control,
        scale_for: impl Fn(usize) -> Scale + Sync,
    ) -> Result<Vec<Hit>> {
        control.check()?;
        let mut ranked = self
            .entries
            .par_iter()
            .map(|entry| {
                control.check()?;
                Ok((
                    entry,
                    entry.descriptor.distance(query, scale_for(entry.asset)),
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        ranked.par_sort_unstable_by(|(a, da), (b, db)| {
            da.total_cmp(db)
                .then_with(|| (a.asset, a.frame, a.region).cmp(&(b.asset, b.frame, b.region)))
        });
        let mut seen = std::collections::BTreeSet::new();
        let hits = ranked
            .into_iter()
            .filter(|(entry, _)| seen.insert(entry.asset))
            .map(|(entry, distance)| Hit {
                asset: entry.asset,
                frame: entry.frame,
                region: entry.region,
                distance,
            })
            .collect();
        control.check()?;
        Ok(hits)
    }
}

impl Entry {
    fn read(plan: &BuildPlan, asset: usize, control: &Control) -> Result<Vec<Self>> {
        let texture = &plan.textures[asset];
        let image = textures::decode(&plan.game.read(&texture.path)?)?;
        ensure!(
            image.dimensions() == (texture.width, texture.height),
            "Texture dimensions changed: {}",
            texture.path
        );
        ensure!(
            texture.frames > 0 && texture.frames <= image.width(),
            "Invalid frame layout: {}",
            texture.path
        );
        let width = image.width() / texture.frames;
        let mut entries = Vec::new();
        for frame in 0..texture.frames {
            control.check()?;
            let region = Region {
                x: frame * width,
                y: 0,
                width,
                height: image.height(),
            };
            let Some(trimmed) = region.opaque_bounds(&image) else {
                continue;
            };
            for candidate in [Some(region), (trimmed != region).then_some(trimmed)]
                .into_iter()
                .flatten()
            {
                entries.push(Self {
                    asset,
                    frame,
                    region: candidate,
                    descriptor: Descriptor::new(&image, candidate),
                });
            }
        }
        Ok(entries)
    }
}

impl Hit {
    pub fn thumbnail(&self, plan: &BuildPlan) -> Result<RgbaImage> {
        let image = textures::decode(&plan.game.read(&plan.textures[self.asset].path)?)?;
        let r = self.region;
        ensure!(
            r.x + r.width <= image.width() && r.y + r.height <= image.height(),
            "Texture dimensions changed during search"
        );
        let ratio = (96.0 / f64::from(r.width))
            .min(72.0 / f64::from(r.height))
            .min(1.0);
        let width = (f64::from(r.width) * ratio).round().max(1.0) as u32;
        let height = (f64::from(r.height) * ratio).round().max(1.0) as u32;
        let view = image.view(r.x, r.y, r.width, r.height);
        Ok(imageops::resize(
            &*view,
            width,
            height,
            imageops::FilterType::Triangle,
        ))
    }
}

#[cfg(test)]
mod tests;
