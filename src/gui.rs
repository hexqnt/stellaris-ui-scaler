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
mod preview;
mod project;
mod settings;
mod source_mods;
mod texture_rules;
mod theme;

use std::{
    path::PathBuf,
    sync::{Arc, mpsc},
    time::Duration,
};

use anyhow::{Result, anyhow};
use eframe::egui::{self, TextureHandle, TextureOptions, Vec2};

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
    Finished(Result<Completed, String>),
}

enum Completed {
    Plan(Box<compiler::EnabledModScan>),
    Preview(compiler::TexturePreview),
    Built {
        output: PathBuf,
        mods_directory: Option<PathBuf>,
    },
}
struct Studio {
    tab: WorkspaceTab,
    font_settings: font_settings::FontSettings,
    source_mods_editor: source_mods::Editor,
    game: String,
    game_locations: Vec<game_locations::GameLocation>,
    output: String,
    built_mod: Option<PathBuf>,
    mods_directory: String,
    profile_path: String,
    profile: Profile,
    plan: Option<Arc<BuildPlan>>,
    selected: Option<usize>,
    filter: String,
    category: Option<Category>,
    asset_search: AssetSearch,
    asset_tree: AssetTree,
    rule_selection: Option<Selection>,
    preview_dirty: bool,
    preview_features: Option<crate::config::ImageFeatures>,
    images: Vec<(String, TextureHandle)>,
    receiver: Option<mpsc::Receiver<Event>>,
    control: Control,
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
            mods_directory: String::new(),
            profile_path: "profiles/4k.toml".into(),
            profile,
            plan: None,
            selected: None,
            filter: String::new(),
            category: None,
            asset_search: AssetSearch::default(),
            asset_tree: AssetTree::default(),
            rule_selection: None,
            preview_dirty: false,
            preview_features: None,
            images: Vec::new(),
            receiver: None,
            control: Control::default(),
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
        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);
        self.control = Control::default();
        let control = self.control.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = work(control, &sender).map_err(|error| format!("{error:#}"));
            let _ = sender.send(Event::Finished(result));
            ctx.request_repaint();
        });
    }
    fn poll(&mut self, ctx: &egui::Context) {
        while let Some(receiver) = &self.receiver {
            let event = match receiver.try_recv() {
                Ok(event) => event,
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.receiver = None;
                    self.status = "Worker stopped unexpectedly".into();
                    break;
                }
            };
            match event {
                Event::Progress(p) => {
                    self.status = format!("{}: {}/{}", p.stage, p.completed, p.total);
                }
                Event::Finished(result) => {
                    self.receiver = None;
                    match result {
                        Ok(Completed::Plan(scan)) => self.accept_scan(*scan),
                        Ok(Completed::Preview(preview)) => {
                            self.preview_features = preview.features;
                            self.select_current_algorithm();
                            self.images = preview
                                .images
                                .into_iter()
                                .map(|(name, image)| {
                                    let size = [image.width() as usize, image.height() as usize];
                                    let handle = ctx.load_texture(
                                        &name,
                                        egui::ColorImage::from_rgba_unmultiplied(
                                            size,
                                            image.as_raw(),
                                        ),
                                        TextureOptions::NEAREST,
                                    );
                                    (name, handle)
                                })
                                .collect();
                            self.status =
                                "Preview ready; select a candidate to save its processing rule"
                                    .into();
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
        if self.receiver.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
    fn selected_asset(&self) -> Option<&TextureAsset> {
        self.plan.as_ref()?.textures.get(self.selected?)
    }
    fn select_current_algorithm(&mut self) {
        if let Some(asset) = self.selected_asset() {
            let algorithm = asset
                .processing(&self.profile, self.preview_features.as_ref())
                .options
                .algorithm;
            self.chosen = self.algorithms[..self.count]
                .iter()
                .position(|&option| option == algorithm)
                .unwrap_or(0);
        }
    }
    fn invalidate_preview(&mut self) {
        self.images.clear();
        self.preview_features = None;
        self.preview_offset = Vec2::ZERO;
        self.preview_dirty = true;
    }
    fn preview(&mut self, ctx: &egui::Context) {
        let (Some(plan), Some(index)) = (self.plan.clone(), self.selected) else {
            return;
        };
        let profile = self.profile.clone();
        let algorithms = self.algorithms;
        let count = self.count;
        self.start(ctx, move |control, _| {
            let asset = &plan.textures[index];
            let images = compiler::preview_candidates_with_analysis(
                &plan,
                &profile,
                asset,
                &algorithms[..count],
                &control,
            )?;
            Ok(Completed::Preview(images))
        });
    }
}

impl Studio {
    fn refresh_asset_filter(&mut self) {
        let textures = self.plan.as_ref().map_or(&[][..], |plan| &plan.textures);
        self.asset_search
            .update(textures, &self.filter, self.category);
        self.asset_tree.update(textures, &self.asset_search.matches);
        if !self.filter.trim().is_empty() {
            self.asset_tree.expand_all();
        }
    }
}

impl Drop for Studio {
    fn drop(&mut self) {
        self.control.cancel();
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
