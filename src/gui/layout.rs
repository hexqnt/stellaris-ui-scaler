use eframe::egui::{self, Align, Color32, Layout, RichText, Vec2};

use super::{Selection, Studio, WorkspaceTab, icons, theme};

impl Studio {
    fn header(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, busy: bool) {
        egui::Panel::top("studio-header")
            .resizable(false)
            .frame(theme::frame(theme::BACKGROUND, 14))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(icons::LOGO).size(27.0).color(theme::MINT));
                    ui.label(RichText::new("Stellaris UI Scaler").size(19.0).strong());
                    ui.add_space(12.0);
                    ui.add_enabled_ui(!busy, |ui| {
                        ui.horizontal(|ui| {
                            ui.weak("Profile");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.profile_path)
                                    .desired_width(180.0),
                            );
                            if ui.add(icons::button(icons::LOAD, "Load")).clicked() {
                                self.load_profile();
                            }
                            if ui.add(icons::button(icons::SAVE, "Save profile")).clicked() {
                                self.save_profile();
                            }
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                self.install_button(ui);
                                self.build_button(ui, ctx);
                            });
                        });
                    });
                });
            });
        egui::Panel::top("project-strip")
            .resizable(false)
            .frame(theme::frame(theme::PANEL, 12))
            .show(ui, |ui| {
                ui.add_enabled_ui(!busy, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        let game_width = (ui.available_width() * 0.3).clamp(300.0, 400.0);
                        let output_width = 200.0;
                        ui.weak("Game");
                        self.game_path_field(ui, game_width);
                        self.find_game_button(ui);
                        self.scan_button(ui, ctx);
                        ui.separator();
                        ui.weak("Output");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.output)
                                .desired_width(output_width),
                        );
                        ui.separator();
                        self.scale_controls(ui);
                    });
                });
                if let Some(warning) = self.plan.as_ref().and_then(|plan| plan.version_warning()) {
                    ui.colored_label(ui.visuals().warn_fg_color, &warning.message);
                }
            });
    }

    fn navigation(&mut self, ui: &mut egui::Ui) {
        egui::Panel::left("navigation")
            .resizable(false)
            .exact_size(106.0)
            .frame(theme::frame(theme::BACKGROUND, 8))
            .show(ui, |ui| {
                for (tab, icon, label) in [
                    (WorkspaceTab::Project, icons::PROJECT, "Project"),
                    (WorkspaceTab::Textures, icons::TEXTURES, "Textures"),
                    (WorkspaceTab::Fonts, icons::FONTS, "Fonts"),
                    (WorkspaceTab::ModDetails, icons::MOD, "Mod details"),
                ] {
                    if navigation_button(ui, self.tab == tab, icon, label).clicked() {
                        self.tab = tab;
                    }
                }
                ui.with_layout(Layout::bottom_up(Align::Center), |ui| {
                    ui.small(env!("CARGO_PKG_VERSION"));
                });
            });
    }

    fn status_bar(&self, ui: &mut egui::Ui, busy: bool) {
        egui::Panel::bottom("status-bar")
            .resizable(false)
            .frame(theme::frame(theme::BACKGROUND, 10))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if busy {
                        ui.spinner();
                        if ui.add(icons::button(icons::CLEAR, "Cancel")).clicked() {
                            self.control.cancel();
                        }
                    } else {
                        ui.label(RichText::new(icons::READY).color(theme::MINT).size(19.0));
                    }
                    let width = (ui.available_width() - 175.0).max(40.0);
                    let message = ui
                        .add_sized(
                            [width, 24.0],
                            egui::Label::new(&self.status)
                                .truncate()
                                .sense(egui::Sense::click()),
                        )
                        .on_hover_text(&self.status)
                        .on_hover_text("Click to copy the full message")
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                    if message.clicked() {
                        ui.ctx().copy_text(self.status.clone());
                    }
                    ui.separator();
                    if let Some(plan) = &self.plan {
                        ui.weak(format!("Stellaris {}", plan.game.version));
                    } else {
                        ui.weak("No installation scanned");
                    }
                });
            });
    }

    fn inspector(&mut self, ui: &mut egui::Ui, busy: bool) {
        egui::ScrollArea::vertical()
            .id_salt("inspector-scroll")
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                let Some(selection) = self.rule_selection.take() else {
                    theme::section(ui, "Texture settings");
                    ui.weak("Select an asset or folder in the browser to edit its settings.");
                    return;
                };
                let asset = self.selected_asset();
                let category = match &selection {
                    Selection::Category(category) => *category,
                    _ => asset.map_or(crate::config::Category::Icon, |asset| asset.category),
                };
                theme::section(
                    ui,
                    match selection {
                        Selection::Asset(_) => "Asset settings",
                        Selection::Directory(_) => "Folder settings",
                        Selection::Category(_) => "Category settings",
                    },
                );
                ui.add(egui::Label::new(selection.key()).wrap());
                match &selection {
                    Selection::Asset(_) => {
                        if let Some(asset) = asset {
                            ui.small(format!(
                                "{} × {} · {} · {} frames",
                                asset.width,
                                asset.height,
                                asset.category.name(),
                                asset.frames
                            ));
                        }
                    }
                    Selection::Directory(_) => {
                        ui.weak("Applies to all textures in this folder.");
                    }
                    Selection::Category(_) => {
                        ui.weak("Applies to all textures in this category.");
                    }
                }
                let frame_size = asset.and_then(crate::compiler::TextureAsset::frame_size);
                ui.separator();
                ui.add_enabled_ui(!busy, |ui| {
                    if selection.show(
                        ui,
                        &mut self.profile,
                        category,
                        frame_size,
                        self.preview_features.as_ref(),
                    ) {
                        self.invalidate_preview();
                    }
                });
                self.rule_selection = Some(selection);
                ui.separator();
                ui.collapsing("Preview filters", |ui| self.candidates_panel(ui, busy));
            });
    }

    fn texture_workspace(&mut self, ui: &mut egui::Ui, busy: bool) {
        egui::Panel::left("asset-browser")
            .default_size(270.0)
            .size_range(200.0..=420.0)
            .frame(theme::frame(theme::PANEL, 12))
            .show(ui, |ui| {
                ui.strong("Asset browser");
                self.assets_panel(ui, busy);
                if self.plan.is_none() {
                    ui.add_space(12.0);
                    ui.weak("Scan an installation to browse textures.");
                }
            });
        egui::Panel::right("asset-inspector")
            .default_size(280.0)
            .size_range(260.0..=400.0)
            .frame(theme::frame(theme::PANEL, 14))
            .show(ui, |ui| self.inspector(ui, busy));
        egui::CentralPanel::default()
            .frame(theme::frame(theme::BACKGROUND, 14))
            .show(ui, |ui| {
                self.preview_toolbar(ui);
                ui.separator();
                egui::Panel::bottom("preview-summary")
                    .resizable(false)
                    .frame(theme::frame(theme::SURFACE, 10))
                    .show(ui, |ui| {
                        if let Some(asset) = self.selected_asset() {
                            let processing =
                                asset.processing(&self.profile, self.preview_features.as_ref());
                            let options = processing.options;
                            ui.horizontal_wrapped(|ui| {
                                ui.small(format!("UI scale {:.2}×", self.profile.scale.get()));
                                ui.separator();
                                ui.small(format!("Effective: {:?}", options.algorithm));
                                ui.separator();
                                ui.small(format!("Sharpening {:.2}", options.sharpening));
                                if self.preview_features.is_none()
                                    && self.profile.needs_pixel_analysis(
                                        &asset.path,
                                        asset.category,
                                        asset.frame_size(),
                                    )
                                {
                                    ui.small("Auto: waiting for pixel analysis");
                                } else if let Some(choice) = processing.recommendation {
                                    ui.small(format!("Auto: {}", choice.reason));
                                }
                            });
                        } else {
                            ui.small(
                                "Texture preview · changes are applied when you build the mod",
                            );
                        }
                    });
                self.previews_panel(ui);
            });
    }
}

impl Studio {
    pub(super) fn show(&mut self, root: &mut egui::Ui) {
        let ctx = root.ctx().clone();
        self.poll(&ctx);
        if ctx.input_mut(|input| {
            input.consume_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers::COMMAND,
                egui::Key::F,
            ))
        }) {
            self.tab = WorkspaceTab::Textures;
            ctx.memory_mut(|memory| memory.request_focus(egui::Id::unique("asset-search")));
        }
        let busy = self.receiver.is_some();
        self.header(root, &ctx, busy);
        self.status_bar(root, busy);
        self.navigation(root);
        match self.tab {
            WorkspaceTab::Textures => self.texture_workspace(root, busy),
            tab => {
                egui::CentralPanel::default()
                    .frame(theme::frame(theme::PANEL, 24))
                    .show(root, |ui| match tab {
                        WorkspaceTab::Project => self.project_page(ui, &ctx, busy),
                        WorkspaceTab::ModDetails => self.mod_page(ui, busy),
                        WorkspaceTab::Fonts => {
                            if let Some(plan) = &self.plan {
                                ui.add_enabled_ui(!busy, |ui| {
                                    self.font_settings.show(ui, &plan.fonts, &mut self.profile);
                                });
                            } else {
                                ui.heading("Fonts");
                                ui.weak("Scan an installation to inspect its font groups.");
                            }
                        }
                        WorkspaceTab::Textures => unreachable!(),
                    });
            }
        }
        if self.preview_dirty
            && self.receiver.is_none()
            && self.selected.is_some()
            && self.tab == WorkspaceTab::Textures
        {
            self.preview_dirty = false;
            self.preview(&ctx);
        }
    }
}

impl eframe::App for Studio {
    fn ui(&mut self, root: &mut egui::Ui, _: &mut eframe::Frame) {
        self.show(root);
    }
}
fn navigation_button(ui: &mut egui::Ui, selected: bool, icon: &str, label: &str) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(88.0, 76.0), egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::Button, ui.is_enabled(), selected, label)
    });
    let fill = if selected {
        theme::MINT
    } else if response.hovered() || response.has_focus() {
        theme::SURFACE
    } else {
        Color32::TRANSPARENT
    };
    let color = if selected {
        theme::INK
    } else {
        ui.visuals().text_color()
    };
    ui.painter().rect_filled(rect, 5.0, fill);
    ui.painter().text(
        rect.center_top() + Vec2::new(0.0, 24.0),
        egui::Align2::CENTER_CENTER,
        icon,
        egui::FontId::proportional(26.0),
        color,
    );
    ui.painter().text(
        rect.center_top() + Vec2::new(0.0, 56.0),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(13.0),
        color,
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}
