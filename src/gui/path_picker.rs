use std::path::{Path, PathBuf};

use eframe::egui;
use egui_file_dialog::{DialogState, FileDialog};

use super::Studio;

#[derive(Clone, Copy)]
pub(super) enum Kind {
    Directory,
    Profile,
    SaveProfile,
    Image,
}

#[derive(Clone, Copy)]
pub(super) enum Target {
    Game,
    Profile,
    SaveProfile,
    Output,
    UserData,
    Mods,
    Picture,
}

pub(super) struct PathEdit(pub Option<PathBuf>);

pub(super) struct Picker<T> {
    active: Option<(T, FileDialog)>,
}

impl<T> Picker<T> {
    pub(super) fn open(&mut self, target: T, kind: Kind, current: &Path, title: &str) {
        let mut dialog = FileDialog::new().as_modal(true).title(title);
        // Несуществующий путь результата или профиля открываем с ближайшей доступной папки.
        if let Some(directory) = current.ancestors().find(|path| path.is_dir()) {
            dialog = dialog.initial_directory(directory.to_owned());
        }
        match kind {
            Kind::Directory => dialog.pick_directory(),
            Kind::Profile => {
                dialog = dialog
                    .add_file_filter_extensions("TOML profiles", vec!["toml"])
                    .default_file_filter("TOML profiles");
                dialog.pick_file();
            }
            Kind::SaveProfile => {
                dialog = dialog
                    .default_file_name(
                        current
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("profile.toml"),
                    )
                    .add_save_extension("TOML profile", "toml")
                    .default_save_extension("TOML profile");
                dialog.save_file();
            }
            Kind::Image => {
                dialog = dialog
                    .add_file_filter_extensions(
                        "Images",
                        vec!["png", "jpg", "jpeg", "dds", "tga", "bmp"],
                    )
                    .default_file_filter("Images");
                dialog.pick_file();
            }
        }
        self.active = Some((target, dialog));
    }

    pub(super) fn show(&mut self, ctx: &egui::Context) -> Option<(T, PathBuf)> {
        let (_, dialog) = self.active.as_mut()?;
        dialog.update(ctx);
        if let Some(path) = dialog.take_picked() {
            let (target, _) = self.active.take()?;
            return Some((target, path));
        }
        if matches!(dialog.state(), DialogState::Cancelled | DialogState::Closed) {
            self.active = None;
        }
        None
    }
}

impl Picker<Target> {
    pub(super) fn field(
        &mut self,
        ui: &mut egui::Ui,
        target: Target,
        value: &mut String,
        width: f32,
        hint: &str,
    ) {
        ui.add(
            egui::TextEdit::singleline(value)
                .hint_text(hint)
                .desired_width(width),
        );
        self.button(ui, target, Path::new(value.trim()));
    }

    pub(super) fn optional_field(
        &mut self,
        ui: &mut egui::Ui,
        target: Target,
        current: Option<&Path>,
        edit: impl FnOnce(&mut egui::Ui, egui::TextEdit<'_>) -> egui::Response,
    ) -> Option<PathEdit> {
        let mut text = current.map_or_default(Path::to_string_lossy);
        let response = edit(ui, egui::TextEdit::singleline(&mut text));
        let updated = response
            .changed()
            .then(|| (!text.is_empty()).then(|| PathBuf::from(text.as_ref())));
        let current = updated.as_ref().map_or(current, |path| path.as_deref());
        self.button(ui, target, current.unwrap_or_else(|| Path::new("")));
        updated.map(PathEdit)
    }

    pub(super) fn button(&mut self, ui: &mut egui::Ui, target: Target, current: &Path) {
        let (kind, title) = match target {
            Target::Game => (Kind::Directory, "Select Stellaris installation"),
            Target::Profile => (Kind::Profile, "Select profile"),
            Target::SaveProfile => (Kind::SaveProfile, "Save profile as"),
            Target::Output => (Kind::Directory, "Select output directory"),
            Target::UserData => (Kind::Directory, "Select Stellaris user data directory"),
            Target::Mods => (Kind::Directory, "Select mods directory"),
            Target::Picture => (Kind::Image, "Select mod picture"),
        };
        let label = if matches!(target, Target::SaveProfile) {
            "Save as…"
        } else {
            "…"
        };
        let button = ui.button(label).on_hover_text(title);
        button.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::Role::Button,
                ui.is_enabled(),
                if matches!(target, Target::SaveProfile) {
                    label
                } else {
                    title
                },
            )
        });
        if button.clicked() {
            self.open(target, kind, current, title);
        }
    }
}

impl<T> Default for Picker<T> {
    fn default() -> Self {
        Self { active: None }
    }
}

impl Studio {
    pub(super) fn path_dialog(&mut self, ctx: &egui::Context) {
        if let Some((target, path)) = self.path_picker.show(ctx) {
            self.accept_path(target, path);
        }
    }

    fn accept_path(&mut self, target: Target, path: PathBuf) {
        match target {
            Target::Game => {
                if Path::new(self.game.trim()) != path {
                    self.game = path.display().to_string();
                    self.game_changed();
                }
            }
            Target::Profile => self.profile_path = path.display().to_string(),
            Target::SaveProfile => {
                self.profile_path = path.display().to_string();
                self.save_profile();
            }
            Target::Output => self.output = path.display().to_string(),
            Target::UserData => {
                self.profile
                    .auto_source_mods
                    .get_or_insert_default()
                    .user_data_dir = Some(path);
                self.invalidate_sources();
                self.status = "Source mods changed; scan assets again".into();
            }
            Target::Mods => self.mods_directory = path.display().to_string(),
            Target::Picture => self.profile.picture = Some(path),
        }
    }
}
