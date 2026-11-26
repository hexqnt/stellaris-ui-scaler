mod descriptor;
mod index;
mod view;
mod worker;

use std::sync::Arc;

use eframe::egui::{self, TextureHandle};
use image::RgbaImage;

use super::Studio;
use crate::{
    compiler::BuildPlan,
    config::{ImageFeatures, Profile},
};
use index::{Hit, Index};
use worker::{Completed, Input, Job};

const PAGE_SIZE: usize = 20;

struct ResultRow {
    hit: Hit,
    texture: TextureHandle,
    features: ImageFeatures,
}

#[derive(Default)]
pub(super) struct VisualSearch {
    pub open: bool,
    path: String,
    picker: super::path_picker::Picker<()>,
    index: Option<Arc<Index>>,
    job: Option<Job>,
    rows: Vec<ResultRow>,
    ranked: Arc<[Hit]>,
    paging_paused: bool,
    generation: u64,
    query: Option<TextureHandle>,
    status: String,
}

impl VisualSearch {
    fn start(&mut self, ctx: &egui::Context, plan: Arc<BuildPlan>, profile: Profile, input: Input) {
        self.job = None;
        self.rows.clear();
        self.ranked = Arc::default();
        self.paging_paused = false;
        self.generation = self.generation.wrapping_add(1);
        self.query = None;
        self.status = if self.index.is_some() {
            "Finding similar textures…"
        } else {
            "Building visual index…"
        }
        .into();
        self.job = Some(Job::search(ctx, plan, profile, input, self.index.clone()));
    }

    fn cancel(&mut self) {
        if self.job.take().is_some() {
            self.paging_paused = true;
            self.status = "Image search cancelled".into();
        }
    }

    fn close(&mut self) {
        self.open = false;
        if self.job.take().is_some() && self.ranked.is_empty() {
            self.status = "Image search cancelled".into();
        }
    }

    fn upload(ctx: &egui::Context, image: &RgbaImage) -> TextureHandle {
        ctx.load_texture(
            "visual-search",
            egui::ColorImage::from_rgba_unmultiplied(
                [image.width() as usize, image.height() as usize],
                image.as_raw(),
            ),
            egui::TextureOptions::LINEAR,
        )
    }

    fn load_next_page(&mut self, ctx: &egui::Context, plan: Arc<BuildPlan>) {
        if self.job.is_some() || self.rows.len() >= self.ranked.len() {
            return;
        }
        let start = self.rows.len();
        let end = start + (self.ranked.len() - start).min(PAGE_SIZE);
        self.job = Some(Job::page(ctx, plan, Arc::clone(&self.ranked), start..end));
        self.paging_paused = false;
    }

    fn poll(&mut self, ctx: &egui::Context) {
        let Some(job) = &self.job else {
            return;
        };
        let Some(result) = job.poll(ctx) else {
            return;
        };
        self.job = None;
        match result {
            Ok(completed) => {
                let page = match completed {
                    Completed::Search(result) => {
                        self.query = Some(Self::upload(ctx, &result.query));
                        self.index = Some(result.index);
                        self.ranked = result.ranked;
                        result.page
                    }
                    Completed::Page(page) => page,
                };
                self.rows
                    .extend(page.into_iter().map(|(hit, preview)| ResultRow {
                        texture: Self::upload(ctx, &preview.image),
                        features: preview.features,
                        hit,
                    }));
                self.status = format!(
                    "Showing {} of {} resources · {} unreadable textures skipped",
                    self.rows.len(),
                    self.ranked.len(),
                    self.index.as_ref().map_or_default(|index| index.skipped)
                );
            }
            Err(error) => {
                self.paging_paused = true;
                self.status = error;
            }
        }
    }
}

impl Studio {
    pub(super) fn visual_search_window(&mut self, ctx: &egui::Context, busy: bool) {
        self.visual_search.poll(ctx);
        let Some(plan) = &self.plan else {
            return;
        };
        let selected = self.selected();
        if let Some(index) = self
            .visual_search
            .show(ctx, plan, &self.profile, selected, busy)
        {
            self.open_asset(index);
        }
    }
}
#[cfg(test)]
mod tests;
