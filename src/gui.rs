// Масштабы и координаты ограничены элементами управления; преобразования служат только отрисовке.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
mod asset_search;
mod asset_tree;
mod browser;
mod font_settings;
mod game_locations;
mod icons;
mod layout;
mod mod_details;
mod mod_install;
mod path_picker;
mod preview;
mod project;
mod settings;
mod source_mods;
mod task;
mod texture_rules;
mod theme;
mod visual_search;

use std::{
    path::PathBuf,
    sync::{Arc, mpsc},
};

use anyhow::{Result, anyhow};
use eframe::egui::{self, TextureHandle, Vec2};

use crate::{
    compiler::{self, BuildPlan, Control, Progress, TextureAsset},
    config::{Algorithm, Category, Profile},
};
use asset_search::AssetSearch;
use asset_tree::AssetTree;
use texture_rules::Selection;

#[derive(Clone, Copy, PartialEq, Eq)]
enum WorkspaceTab {
    Project,
    Textures,
    Fonts,
    ModDetails,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ComparisonMode {
    Compare,
    Ab,
    Split,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum PreviewBackground {
    Checker,
    Dark,
    Light,
}

enum Event {
    Progress(Progress),
    Preview(compiler::PreviewEvent),
    Finished(Result<Completed, String>),
}

enum Completed {
    Plan(Box<compiler::EnabledModScan>),
    Preview,
    Built {
        output: PathBuf,
        mods_directory: Option<PathBuf>,
    },
}
enum PreviewImage {
    Pending,
    Ready(TextureHandle),
    Failed(String),
}

impl PreviewImage {
    const fn handle(&self) -> Option<&TextureHandle> {
        match self {
            Self::Ready(handle) => Some(handle),
            Self::Pending | Self::Failed(_) => None,
        }
    }
}

struct Studio {
    tab: WorkspaceTab,
    font_settings: font_settings::FontSettings,
    source_mods_editor: source_mods::Editor,
    game: String,
    game_locations: Vec<game_locations::GameLocation>,
    output: String,
    built_mod: Option<PathBuf>,
    rebuild_from_scratch: bool,
    mods_directory: String,
    profile_path: String,
    path_picker: path_picker::Picker<path_picker::Target>,
    profile: Profile,
    plan: Option<Arc<BuildPlan>>,
    filter: String,
    category: Option<Category>,
    asset_search: AssetSearch,
    asset_tree: AssetTree,
    visual_search: visual_search::VisualSearch,
    rule_selection: Option<Selection>,
    preview_dirty: bool,
    preview_features: Option<crate::config::ImageFeatures>,
    images: Vec<(String, PreviewImage)>,
    preview_session: Arc<compiler::PreviewSession>,
    job: Option<task::Task<Event>>,
    status: String,
    zoom: f32,
    preview_offset: Vec2,
    count: usize,
    fit_preview: bool,
    chosen: usize,
    algorithms: [Algorithm; 4],
    mode: ComparisonMode,
    split: f32,
    background: PreviewBackground,
}
impl Studio {
    fn new(game: Option<PathBuf>) -> Self {
        let game_locations = game_locations::discover();
        let game = game.or_else(|| {
            game_locations
                .first()
                .map(|location| location.path().to_owned())
        });
        let profile = Profile::load(std::path::Path::new("profiles/4k.toml")).unwrap_or_default();
        Self {
            tab: WorkspaceTab::Textures,
            font_settings: font_settings::FontSettings::for_profile(&profile),
            source_mods_editor: source_mods::Editor::default(),
            game: game.map_or_default(|path| path.display().to_string()),
            game_locations,
            output: "dist/stellaris-ui-scaled".into(),
            built_mod: None,
            rebuild_from_scratch: false,
            mods_directory: String::new(),
            profile_path: "profiles/4k.toml".into(),
            path_picker: path_picker::Picker::default(),
            profile,
            plan: None,
            filter: String::new(),
            category: None,
            asset_search: AssetSearch::default(),
            asset_tree: AssetTree::default(),
            visual_search: visual_search::VisualSearch::default(),
            rule_selection: None,
            preview_dirty: false,
            preview_features: None,
            images: Vec::new(),
            preview_session: Arc::default(),
            job: None,
            status: "Select an installation and scan its assets".into(),
            zoom: 1.0,
            preview_offset: Vec2::ZERO,
            count: 4,
            fit_preview: false,
            chosen: 0,
            algorithms: [
                Algorithm::Lanczos3,
                Algorithm::CatmullRom,
                Algorithm::Mitchell,
                Algorithm::Lanczos2,
            ],
            mode: ComparisonMode::Compare,
            split: 0.5,
            background: PreviewBackground::Checker,
        }
    }
    fn start(
        &mut self,
        ctx: &egui::Context,
        work: impl FnOnce(Control, &mpsc::Sender<Event>) -> Result<Completed> + Send + 'static,
    ) {
        self.job = Some(task::Task::spawn(ctx, move |control, sender| {
            Event::Finished(work(control, sender).map_err(|error| format!("{error:#}")))
        }));
    }
    fn poll(&mut self, ctx: &egui::Context) {
        while let Some(job) = &self.job {
            let event = match job.try_recv(ctx) {
                Ok(event) => event,
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.job = None;
                    self.status = "Worker stopped unexpectedly".into();
                    break;
                }
            };
            match event {
                Event::Preview(event) => self.accept_preview(ctx, event),
                Event::Progress(p) => {
                    self.status = p.to_string();
                }
                Event::Finished(result) => {
                    self.job = None;
                    if let Err(error) = &result {
                        for (_, image) in &mut self.images {
                            if matches!(image, PreviewImage::Pending) {
                                *image = PreviewImage::Failed(error.clone());
                            }
                        }
                    }
                    match result {
                        Ok(Completed::Plan(scan)) => self.accept_scan(*scan),
                        Ok(Completed::Preview) => {
                            self.status = "Preview complete; select a ready candidate to save its processing rule".into();
                        }
                        Ok(Completed::Built {
                            output,
                            mods_directory,
                        }) => {
                            self.built_mod = Some(output);
                            if self.mods_directory.is_empty() {
                                self.mods_directory = mods_directory
                                    .map_or_default(|path| path.display().to_string());
                            }
                            self.status = "Build complete; in-game validation is pending".into();
                        }
                        Err(error) => self.status = error,
                    }
                }
            }
        }
    }
    const fn selected(&self) -> Option<usize> {
        match self.rule_selection {
            Some(Selection::Asset(index)) => Some(index),
            _ => None,
        }
    }

    fn assets(&self) -> &[TextureAsset] {
        self.plan
            .as_ref()
            .map_or_default(|plan| plan.textures.as_slice())
    }

    fn selected_asset(&self) -> Option<&TextureAsset> {
        self.assets().get(self.selected()?)
    }
    fn current_algorithm_candidate(&self) -> Option<usize> {
        let algorithm = self
            .selected_asset()?
            .processing(&self.profile, self.preview_features.as_ref())
            .options
            .algorithm;
        self.algorithms[..self.count]
            .iter()
            .position(|&option| option == algorithm)
    }
    fn select_current_algorithm(&mut self) {
        self.chosen = self.current_algorithm_candidate().unwrap_or(0);
    }
    fn invalidate_preview(&mut self) {
        self.images.clear();
        self.preview_features = None;
        self.preview_offset = Vec2::ZERO;
        self.preview_dirty = true;
    }
    fn preview(&mut self, ctx: &egui::Context) {
        let (Some(plan), Some(index)) = (self.plan.clone(), self.selected()) else {
            return;
        };
        let profile = self.profile.clone();
        let algorithms = self.algorithms;
        let count = self.count;
        let session = Arc::clone(&self.preview_session);
        let repaint = ctx.clone();
        self.images.clear();
        self.status = "Preparing previews…".into();
        self.start(ctx, move |control, sender| {
            session.render(
                &plan,
                &profile,
                &plan.textures[index],
                &algorithms[..count],
                &control,
                &|event| {
                    let _ = sender.send(Event::Preview(event));
                    repaint.request_repaint();
                },
            )?;
            Ok(Completed::Preview)
        });
    }
}

impl Studio {
    fn refresh_asset_filter(&mut self) {
        let textures = self
            .plan
            .as_ref()
            .map_or_default(|plan| plan.textures.as_slice());
        self.asset_search
            .update(textures, &self.filter, self.category);
        self.asset_tree.update(textures, &self.asset_search.matches);
        if !self.filter.trim().is_empty() {
            self.asset_tree.expand_all();
        }
    }
}

/// # Errors
/// Возвращает ошибку создания окна или графического контекста.
pub fn run(game: Option<PathBuf>) -> Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1600.0, 1000.0])
            .with_min_inner_size([1100.0, 700.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Stellaris UI Scaler",
        options,
        Box::new(move |context| {
            theme::install(&context.egui_ctx);
            icons::install(&context.egui_ctx)?;
            Ok(Box::new(Studio::new(game)))
        }),
    )
    .map_err(|e| anyhow!("{e}"))
}
#[cfg(test)]
mod tests;
