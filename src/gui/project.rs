use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use eframe::egui::{self, Vec2};

use crate::{compiler, config::Profile};

use super::{Completed, Event, Studio, icons, mod_install, settings, theme};

impl Studio {
    pub(super) fn scale_controls(&mut self, ui: &mut egui::Ui) {
        let previous = (self.profile.scale, self.profile.outliner.compact);
        ui.horizontal_wrapped(|ui| {
            ui.label("Scale");
            settings::scale(ui, &mut self.profile.scale, "");
            ui.checkbox(&mut self.profile.outliner.compact, "Compact outliner");
        });
        if previous != (self.profile.scale, self.profile.outliner.compact) {
            self.invalidate_preview();
        }
    }
    pub(super) fn processing_controls(&mut self, ui: &mut egui::Ui) {
        let previous = (
            self.profile.textures,
            self.profile.auto_algorithm,
            self.profile.auto_sharpening,
        );
        egui::Grid::new("processing-fields")
            .spacing([8.0, 4.0])
            .show(ui, |ui| {
                ui.label(if self.profile.auto_algorithm { "Fallback filter" } else { "Filter" });
                ui.horizontal(|ui| {
                    settings::algorithm(ui, "default-algorithm", 100.0, &mut self.profile.textures.algorithm);
                    ui.checkbox(&mut self.profile.auto_algorithm, "Auto algorithm")
                        .on_hover_text("Choose by purpose, scale, frame size and pixel statistics. Explicit rules take priority.");
                });
                ui.end_row();
                ui.label("Color space");
                ui.horizontal(|ui| {
                    settings::color_space(ui, "color-space", 100.0, &mut self.profile.textures.color_space);
                    ui.checkbox(&mut self.profile.textures.premultiplied_alpha, "Premultiplied alpha");
                    ui.checkbox(&mut self.profile.textures.independent_frames, "Separate frames");
                });
                ui.end_row();
                ui.label("Sharpening");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut self.profile.textures.sharpening).range(0.0..=2.0).speed(0.01));
                    ui.checkbox(&mut self.profile.auto_sharpening, "Auto sharpening")
                        .on_hover_text("Limit sharpening for smooth gradients and strong edges. Explicit sharpening rules take priority.");
                    ui.label("Radius");
                    ui.add(egui::DragValue::new(&mut self.profile.textures.sharpening_radius).range(0.1..=5.0).speed(0.1));
                    ui.label("Threshold");
                    ui.add(egui::DragValue::new(&mut self.profile.textures.sharpening_threshold).range(0..=255));
                });
                ui.end_row();
            });
        if previous
            != (
                self.profile.textures,
                self.profile.auto_algorithm,
                self.profile.auto_sharpening,
            )
        {
            self.invalidate_preview();
        }
    }
    pub(super) fn build_button(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if ui
            .add_enabled(
                self.plan.is_some(),
                theme::primary(icons::BUILD, "Build mod"),
            )
            .clicked()
            && let Some(plan) = self.plan.clone()
        {
            let profile = self.profile.clone();
            let output = PathBuf::from(&self.output);
            self.built_mod = None;
            self.start(ctx, move |control, sender| {
                let report = compiler::build(&plan, &profile, &output, &control, &mut |p| {
                    let _ = sender.send(Event::Progress(p));
                })?;
                Ok(Completed::Built {
                    output: report.output,
                    mods_directory: mod_install::suggested_directory(&plan.game.root).ok(),
                })
            });
        }
    }
    pub(super) fn game_path_field(&mut self, ui: &mut egui::Ui, width: f32) -> egui::Response {
        let input_rect = std::cell::Cell::new(egui::Rect::NOTHING);
        let padding = ui.spacing().button_padding;
        let input_width = (2.0_f32.mul_add(-padding.x, width)
            - ui.spacing().icon_width
            - ui.spacing().item_spacing.x)
            .max(40.0);
        let input_size = Vec2::new(input_width, ui.text_style_height(&egui::TextStyle::Body));
        // ComboBox размещает область редактирования и управляет стандартным меню.
        let input_slot = egui::Atom::paint(input_size, |_, args| input_rect.set(args.rect));
        let mut chosen = None;
        let combo = egui::ComboBox::from_id_salt("game-location")
            .width(width)
            .height(240.0)
            .selected_text(input_slot)
            .show_ui(ui, |ui| {
                if self.game_locations.is_empty() {
                    ui.add(
                        egui::Label::new("No installations found; use Find game or enter a path")
                            .truncate(),
                    );
                }
                for location in &self.game_locations {
                    let path = location.path().display().to_string();
                    let selected = Path::new(self.game.trim()) == location.path();
                    if ui
                        .add(egui::Button::selectable(selected, &path).truncate())
                        .on_hover_text(&path)
                        .clicked()
                    {
                        chosen = Some(path);
                    }
                }
            });
        if let Some(path) = chosen {
            self.game = path;
        }
        ui.place(
            egui::Rect::from_center_size(
                input_rect.get().center(),
                Vec2::new(input_width, combo.response.rect.height()),
            ),
            egui::TextEdit::singleline(&mut self.game)
                .id(combo.response.id.with("input"))
                .hint_text("Stellaris installation directory")
                .frame(egui::Frame::NONE)
                .margin(egui::Margin::ZERO)
                .align(egui::Align2::LEFT_CENTER)
                .desired_width(input_width),
        );
        combo.response
    }

    pub(super) fn find_game_button(&mut self, ui: &mut egui::Ui) {
        if ui.button("Find game").clicked() {
            self.game_locations = super::game_locations::discover();
            self.status = format!("Found {} game installations", self.game_locations.len());
        }
    }

    pub(super) fn scan_button(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if ui.add(icons::button(icons::SCAN, "Scan assets")).clicked() {
            self.scan_assets(ctx);
        }
    }

    pub(super) fn scan_assets(&mut self, ctx: &egui::Context) {
        let game = PathBuf::from(&self.game);
        let profile = self.profile.clone();
        self.source_mods_editor.reset();
        self.invalidate_sources();
        self.status = "Scanning game assets and enabled UI mods".into();
        self.start(ctx, move |control, sender| {
            let scan = compiler::scan_enabled_mods(&game, &profile, &control, &mut |progress| {
                let _ = sender.send(Event::Progress(progress));
            })?;
            Ok(Completed::Plan(Box::new(scan)))
        });
    }

    pub(super) fn accept_scan(&mut self, scan: compiler::EnabledModScan) {
        let compiler::EnabledModScan {
            plan,
            settings,
            changes,
            removed_manual_sources,
        } = scan;
        self.profile.auto_source_mods = Some(settings);
        self.profile.source_mods.clear();
        self.source_mods_editor
            .scanned(changes, removed_manual_sources);
        self.status = format!(
            "{}: {} textures, {} interface files, {} source mods",
            plan.game.version,
            plan.textures.len(),
            plan.sources.len(),
            plan.game.source_mods.len()
        );
        if let Some(notice) = self.source_mods_editor.notice() {
            self.status.push_str("; ");
            self.status.push_str(notice);
        }
        self.plan = Some(Arc::new(plan));
        self.refresh_asset_filter();
        self.selected = None;
        self.rule_selection = None;
        self.invalidate_preview();
    }

    pub(super) fn load_profile(&mut self) {
        match Profile::load(Path::new(&self.profile_path)) {
            Ok(profile) => {
                let sources_changed = profile.source_mods != self.profile.source_mods
                    || profile.auto_source_mods != self.profile.auto_source_mods
                    || profile.disabled_source_mods != self.profile.disabled_source_mods;
                self.profile = profile;
                self.source_mods_editor.reset();
                if sources_changed {
                    self.invalidate_sources();
                    self.status = "Profile loaded; scan assets again".into();
                } else {
                    self.built_mod = None;
                    self.invalidate_preview();
                    self.status = "Profile loaded".into();
                }
            }
            Err(error) => self.status = format!("Cannot load profile: {error:#}"),
        }
    }

    fn invalidate_sources(&mut self) {
        self.plan = None;
        self.selected = None;
        self.rule_selection = None;
        self.built_mod = None;
        self.refresh_asset_filter();
        self.invalidate_preview();
    }

    pub(super) fn save_profile(&mut self) {
        self.status = match self.profile.save(Path::new(&self.profile_path)) {
            Ok(()) => "Profile saved".into(),
            Err(error) => format!("Cannot save profile: {error:#}"),
        };
    }

    pub(super) fn project_page(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, busy: bool) {
        egui::ScrollArea::vertical()
            .id_salt("project-page")
            .show(ui, |ui| {
                ui.set_max_width(780.0);
                ui.heading("Project");
                ui.weak("Choose your installation, profile and output directory.");
                ui.add_space(16.0);
                ui.add_enabled_ui(!busy, |ui| {
                    theme::section(ui, "Game installation");
                    ui.horizontal(|ui| {
                        let width = (ui.available_width() - 100.0).max(120.0);
                        self.game_path_field(ui, width);
                        self.find_game_button(ui);
                    });
                    if self.source_mods_editor.show(ui, &mut self.profile) {
                        self.invalidate_sources();
                        self.status = "Source mods changed; scan assets again".into();
                    }
                    self.scan_button(ui, ctx);
                    if let Some(plan) = &self.plan {
                        ui.label(format!(
                            "{} · {} textures · {} interface files",
                            plan.game.version,
                            plan.textures.len(),
                            plan.sources.len()
                        ));
                    }
                    ui.separator();
                    theme::section(ui, "Profile");
                    path_field(ui, &mut self.profile_path, "Path to a TOML profile");
                    ui.horizontal(|ui| {
                        if ui.add(icons::button(icons::LOAD, "Load profile")).clicked() {
                            self.load_profile();
                        }
                        if ui.add(icons::button(icons::SAVE, "Save profile")).clicked() {
                            self.save_profile();
                        }
                    });
                    ui.separator();
                    theme::section(ui, "Output directory");
                    path_field(ui, &mut self.output, "Generated mod directory");
                    ui.small("The generated mod stays in this directory after installation.");
                    ui.separator();
                    theme::section(ui, "UI scaling");
                    self.scale_controls(ui);
                    ui.separator();
                    theme::section(ui, "Default texture processing");
                    ui.weak(
                        "Categories, folders and individual assets can override these defaults.",
                    );
                    self.processing_controls(ui);
                });
            });
    }
}

fn path_field(ui: &mut egui::Ui, value: &mut String, hint: &str) {
    ui.add(
        egui::TextEdit::singleline(value)
            .hint_text(hint)
            .desired_width(ui.available_width()),
    );
}
