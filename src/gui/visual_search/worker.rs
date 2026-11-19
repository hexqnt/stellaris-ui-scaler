use std::{
    ops::Range,
    path::PathBuf,
    sync::{Arc, atomic::AtomicUsize, mpsc},
    time::Duration,
};

use anyhow::{Context, Result, anyhow};
use eframe::egui;
use image::RgbaImage;
use rayon::prelude::*;

use crate::{
    compiler::{BuildPlan, Control},
    config::Profile,
};

use super::{
    PAGE_SIZE,
    descriptor::Query,
    index::{AssetPreview, Hit, Index},
};

type Page = Vec<(Hit, AssetPreview)>;

pub(super) enum Completed {
    Search(SearchResult),
    Page(Page),
}

pub(super) enum Progress {
    Indexing(Arc<AtomicUsize>),
    Searching,
    Page,
}

pub(super) enum Input {
    File(PathBuf),
    Image(Arc<egui::ColorImage>),
}

impl Input {
    fn load(self) -> Result<RgbaImage> {
        match self {
            Self::File(path) => crate::textures::decode(
                &std::fs::read(&path).with_context(|| format!("Cannot read {}", path.display()))?,
            ),
            Self::Image(image) => {
                let width = u32::try_from(image.size[0])?;
                let height = u32::try_from(image.size[1])?;
                let bytes = image
                    .pixels
                    .iter()
                    .flat_map(egui::Color32::to_srgba_unmultiplied)
                    .collect();
                RgbaImage::from_raw(width, height, bytes)
                    .ok_or_else(|| anyhow!("Invalid clipboard image"))
            }
        }
    }
}

pub(super) struct SearchResult {
    pub index: Arc<Index>,
    pub query: RgbaImage,
    pub ranked: Arc<[Hit]>,
    pub page: Page,
}

pub(super) struct Job {
    receiver: mpsc::Receiver<Result<Completed, String>>,
    pub control: Control,
    pub progress: Progress,
}

impl Job {
    fn spawn(
        ctx: &egui::Context,
        progress: Progress,
        work: impl FnOnce(&Control) -> Result<Completed> + Send + 'static,
    ) -> Self {
        let (sender, receiver) = mpsc::channel();
        let control = Control::default();
        let worker_control = control.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = (|| {
                worker_control.check()?;
                let completed = work(&worker_control)?;
                worker_control.check()?;
                Ok(completed)
            })()
            .map_err(|error: anyhow::Error| format!("{error:#}"));
            let _ = sender.send(result);
            ctx.request_repaint();
        });
        Self {
            receiver,
            control,
            progress,
        }
    }

    pub fn search(
        ctx: &egui::Context,
        plan: Arc<BuildPlan>,
        profile: Profile,
        input: Input,
        cached: Option<Arc<Index>>,
    ) -> Self {
        let completed = Arc::default();
        let progress = if cached.is_some() {
            Progress::Searching
        } else {
            Progress::Indexing(Arc::clone(&completed))
        };
        Self::spawn(ctx, progress, move |control| {
            let image = input.load()?;
            control.check()?;
            let query = Query::new(&image)?;
            let index = match cached {
                Some(index) => index,
                None => Arc::new(Index::build(&plan, control, &completed)?),
            };
            let ranked: Arc<[Hit]> = index
                .search(&query, control, |asset| {
                    plan.textures[asset].policy(&profile).scale
                })?
                .into();
            let page = thumbnails(&ranked[..ranked.len().min(PAGE_SIZE)], &plan, control)?;
            let query = image::DynamicImage::ImageRgba8(image)
                .thumbnail(240, 120)
                .into_rgba8();
            Ok(Completed::Search(SearchResult {
                index,
                query,
                ranked,
                page,
            }))
        })
    }

    pub fn page(
        ctx: &egui::Context,
        plan: Arc<BuildPlan>,
        ranked: Arc<[Hit]>,
        range: Range<usize>,
    ) -> Self {
        Self::spawn(ctx, Progress::Page, move |control| {
            thumbnails(&ranked[range], &plan, control).map(Completed::Page)
        })
    }

    pub fn poll(&self, ctx: &egui::Context) -> Option<Result<Completed, String>> {
        match self.receiver.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(Duration::from_millis(100));
                None
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err("Image search worker stopped unexpectedly".into()))
            }
        }
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        self.control.cancel();
    }
}

fn thumbnails(hits: &[Hit], plan: &BuildPlan, control: &Control) -> Result<Page> {
    hits.par_iter()
        .map(|&hit| {
            control.check()?;
            Ok((hit, hit.preview(plan)?))
        })
        .collect()
}
