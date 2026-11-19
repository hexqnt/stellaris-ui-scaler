use std::{
    borrow::Cow,
    path::{Path, PathBuf},
};

use eframe::egui;

use super::{Studio, WorkspaceTab, icons, mod_install, path_picker::Target, theme};

impl Studio {
    fn picture_controls(&mut self, ui: &mut egui::Ui, width: f32) {
        ui.label("Picture");
        ui.horizontal(|ui| {
            let mut picture = self
                .profile
                .picture
                .as_ref()
                .map_or_default(|path| path.to_string_lossy());
            if ui
                .add(
                    egui::TextEdit::singleline(&mut picture)
                        .hint_text("Image path (optional)")
                        .desired_width((width - 80.0).max(100.0)),
                )
                .on_hover_text(
                    "Saved in the profile; rebuild and reinstall to update the mod picture",
                )
                .changed()
            {
                self.profile.picture =
                    (!picture.is_empty()).then(|| PathBuf::from(picture.as_ref()));
            }
            self.path_picker.button(
                ui,
                Target::Picture,
                self.profile
                    .picture
                    .as_deref()
                    .unwrap_or_else(|| Path::new("")),
            );
            if ui.button("Clear").clicked() {
                self.profile.picture = None;
            }
        });
    }
    fn mod_controls(&mut self, ui: &mut egui::Ui) {
        let width = 300.0;
        egui::Grid::new("mod-fields")
            .spacing([8.0, 4.0])
            .show(ui, |ui| {
                ui.label("Name");
                ui.add(egui::TextEdit::singleline(&mut self.profile.name).desired_width(width))
                    .on_hover_text(
                        "Saved in the profile; rebuild and reinstall to update the launcher name",
                    );
                ui.end_row();
                ui.label("Version");
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.profile.mod_version)
                            .desired_width(70.0),
                    )
                    .on_hover_text("Mod version saved in the profile and shown in the launcher");
                    ui.label("Game").on_hover_text("Supported game version");
                    let detected = self.plan.as_ref().map(|plan| plan.game.version.as_str());
                    let mut version = self
                        .profile
                        .supported_version
                        .as_deref()
                        .or(detected)
                        .map_or_default(Cow::Borrowed);
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut version)
                                .hint_text("Auto")
                                .desired_width(90.0),
                        )
                        .changed()
                    {
                        self.profile.supported_version = Some(version.into_owned());
                    }
                    if ui
                        .button("Use detected")
                        .on_hover_text("Use the game version from the scanned assets")
                        .clicked()
                    {
                        self.profile.supported_version = None;
                    }
                });
                ui.end_row();
                self.picture_controls(ui, width);
                ui.end_row();
            });
    }
    fn install_controls(&mut self, ui: &mut egui::Ui) {
        if let Some(built) = &self.built_mod {
            ui.horizontal(|ui| {
                ui.label("Mods directory");
                ui.add(
                    egui::TextEdit::singleline(&mut self.mods_directory)
                        .hint_text("Absolute path to Stellaris user data/mod"),
                );
                self.path_picker
                    .button(ui, Target::Mods, Path::new(self.mods_directory.trim()));
                if ui
                    .add_enabled(
                        !self.mods_directory.trim().is_empty(),
                        egui::Button::new("Install mod"),
                    )
                    .on_hover_text("Copy the launcher descriptor into the mods directory")
                    .clicked()
                {
                    self.status = install_mod(built, &self.mods_directory);
                }
            });
            ui.label(format!(
                "Mod files: {}. Keep this directory after installation.",
                built.display()
            ));
        }
    }
    pub(super) fn install_button(&mut self, ui: &mut egui::Ui) {
        if ui
            .add_enabled(
                self.built_mod.is_some(),
                icons::button(icons::INSTALL, "Install mod"),
            )
            .on_hover_text("Register the last successful build in the Stellaris mods directory")
            .clicked()
        {
            if self.mods_directory.trim().is_empty() {
                self.tab = WorkspaceTab::ModDetails;
                self.status = "Set the mods directory to install the last build".into();
            } else if let Some(built) = &self.built_mod {
                self.status = install_mod(built, &self.mods_directory);
            }
        }
    }

    pub(super) fn mod_page(&mut self, ui: &mut egui::Ui, busy: bool) {
        egui::ScrollArea::vertical().id_salt("mod-page").show(ui, |ui| {
            ui.set_max_width(850.0);
            ui.heading("Mod details");
            ui.weak("Metadata shown in the Stellaris launcher.");
            ui.add_space(18.0);
            ui.add_enabled_ui(!busy, |ui| {
                self.mod_controls(ui);
                ui.add_space(12.0);
                ui.separator();
                theme::section(ui, "Installation");
                if self.built_mod.is_some() {
                    self.install_controls(ui);
                } else {
                    ui.label("Build the mod to enable installation.");
                    ui.small("After installing, restart the launcher and enable the mod in your playset.");
                }
            });
        });
    }
}

fn install_mod(output: &Path, mods_directory: &str) -> String {
    match mod_install::install(output, Path::new(mods_directory.trim())) {
        Ok(path) => format!("Installed: {}", path.display()),
        Err(error) => format!("Installation failed: {error:#}"),
    }
}
