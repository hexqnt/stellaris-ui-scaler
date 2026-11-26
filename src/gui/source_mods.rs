use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    path::PathBuf,
};

use eframe::egui::{self, Align, Layout};
use egui_extras::{Column, TableBuilder};

use crate::{config::Profile, source_mods};

use super::path_picker::{PathEdit, Picker, Target};

#[derive(Default)]
struct ScanNotice {
    changes: Option<source_mods::ModListMismatch>,
    removed_manual: bool,
}

impl ScanNotice {
    const fn message(&self) -> Option<&'static str> {
        if self.changes.is_some() {
            Some("Enabled UI mods changed; the current list was loaded.")
        } else if self.removed_manual {
            Some("Only enabled launcher mods were loaded; manual folders removed.")
        } else {
            None
        }
    }
}

impl fmt::Display for ScanNotice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(changes) = &self.changes {
            write!(f, "{}", changes.orders())?;
            if self.removed_manual {
                write!(
                    f,
                    "Manual folders were removed; only enabled launcher mods are used."
                )?;
            }
        } else if self.removed_manual {
            write!(
                f,
                "Mod selection is now controlled by checkboxes in the enabled mod list."
            )?;
        }
        Ok(())
    }
}

#[derive(Default)]
pub(super) struct Editor {
    names: BTreeMap<PathBuf, String>,
    notice: ScanNotice,
}

impl Editor {
    pub(super) fn reset(&mut self) {
        self.names.clear();
        self.notice = ScanNotice::default();
    }

    pub(super) fn scanned(
        &mut self,
        changes: Option<source_mods::ModListMismatch>,
        removed_manual: bool,
    ) {
        self.names.clear();
        self.notice = ScanNotice {
            changes,
            removed_manual,
        };
    }

    pub(super) const fn notice(&self) -> Option<&'static str> {
        self.notice.message()
    }

    pub(super) fn show(
        &mut self,
        ui: &mut egui::Ui,
        profile: &mut Profile,
        picker: &mut Picker<Target>,
    ) -> bool {
        let mut changed = false;
        ui.add_space(12.0);
        ui.strong("Source UI mods");
        ui.small("Scan assets lists enabled UI mods. Check the mods you want to include; new mods are unchecked.");
        let paths = profile
            .auto_source_mods
            .as_ref()
            .map_or_default(|settings| settings.source_mods.as_slice());
        for path in paths {
            if !self.names.contains_key(path) {
                self.names.insert(path.clone(), source_mods::mod_name(path));
            }
        }
        changed |= self.table(ui, paths, &mut profile.disabled_source_mods);
        if paths.is_empty() {
            ui.weak("No UI mods listed. Scan assets to read the launcher's enabled mods.");
        }
        ui.small("Later rows override matching files in earlier rows. Save the profile to keep your selection.");
        if let Some(message) = self.notice() {
            ui.colored_label(ui.visuals().warn_fg_color, message)
                .on_hover_ui(|ui| {
                    ui.label(self.notice.to_string());
                });
        }
        ui.collapsing("User data directory", |ui| {
            ui.horizontal(|ui| {
                let directory = profile
                    .auto_source_mods
                    .as_ref()
                    .and_then(|settings| settings.user_data_dir.as_deref());
                if let Some(PathEdit(directory)) =
                    picker.optional_field(ui, Target::UserData, directory, |ui, edit| {
                        ui.add(edit.hint_text("Auto").desired_width(540.0))
                    })
                {
                    profile
                        .auto_source_mods
                        .get_or_insert_default()
                        .user_data_dir = directory;
                    changed = true;
                }
            });
            ui.small("Uses dlc_load.json from the launcher; it may reflect a previous launch.");
        });
        changed
    }

    fn table(
        &self,
        ui: &mut egui::Ui,
        paths: &[PathBuf],
        disabled: &mut BTreeSet<PathBuf>,
    ) -> bool {
        let mut changed = false;
        let row_height = ui.spacing().interact_size.y.max(28.0);
        TableBuilder::new(ui)
            .id_salt("source-mods-table")
            .striped(true)
            .resizable(true)
            .cell_layout(Layout::left_to_right(Align::Center))
            .column(Column::exact(30.0))
            .column(Column::initial(230.0).range(100.0..=400.0).clip(true))
            .column(Column::remainder().clip(true))
            .min_scrolled_height(0.0)
            .max_scroll_height(240.0)
            .header(24.0, |mut header| {
                header.col(|ui| {
                    ui.strong("Use");
                });
                header.col(|ui| {
                    ui.strong("Mod name");
                });
                header.col(|ui| {
                    ui.strong("Path");
                });
            })
            .body(|mut body| {
                for path in paths {
                    let name = &self.names[path];
                    body.row(row_height, |mut row| {
                        let mut selected = !disabled.contains(path);
                        let mut checkbox = None;
                        row.col(|ui| {
                            checkbox = Some(ui.add(egui::Checkbox::without_text(&mut selected)));
                        });
                        row.col(|ui| {
                            let label = ui
                                .add(egui::Label::new(name).truncate())
                                .on_hover_text(name);
                            let checkbox = checkbox.take().unwrap().labelled_by(label.id);
                            if checkbox.changed() {
                                if selected {
                                    disabled.remove(path);
                                } else {
                                    disabled.insert(path.clone());
                                }
                                changed = true;
                            }
                        });
                        row.col(|ui| {
                            let text = path.to_string_lossy();
                            ui.add(egui::Label::new(text.as_ref()).truncate())
                                .on_hover_text(text.as_ref());
                        });
                    });
                }
            });
        changed
    }
}

#[cfg(test)]
mod tests;
