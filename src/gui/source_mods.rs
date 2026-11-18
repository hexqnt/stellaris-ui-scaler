use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
    fmt,
    path::{Path, PathBuf},
};

use eframe::egui::{self, Align, Layout};
use egui_extras::{Column, TableBuilder};

use crate::{config::Profile, source_mods};

use super::path_picker::{Picker, Target};

enum ScanNotice {
    ModsChanged {
        changes: source_mods::ModListMismatch,
        removed_manual: bool,
    },
    ManualSourcesRemoved,
}

impl ScanNotice {
    const fn message(&self) -> &'static str {
        match self {
            Self::ModsChanged { .. } => "Enabled UI mods changed; the current list was loaded.",
            Self::ManualSourcesRemoved => {
                "Only enabled launcher mods were loaded; manual folders removed."
            }
        }
    }
}

impl fmt::Display for ScanNotice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ModsChanged {
                changes,
                removed_manual,
            } => {
                write!(f, "{}", changes.orders())?;
                if *removed_manual {
                    write!(
                        f,
                        "Manual folders were removed; only enabled launcher mods are used."
                    )?;
                }
                Ok(())
            }
            Self::ManualSourcesRemoved => write!(
                f,
                "Mod selection is now controlled by checkboxes in the enabled mod list."
            ),
        }
    }
}

#[derive(Default)]
pub(super) struct Editor {
    names: BTreeMap<PathBuf, String>,
    notice: Option<ScanNotice>,
}

impl Editor {
    pub(super) fn reset(&mut self) {
        self.names.clear();
        self.notice = None;
    }

    pub(super) fn scanned(
        &mut self,
        changes: Option<source_mods::ModListMismatch>,
        removed_manual: bool,
    ) {
        self.reset();
        self.notice = match changes {
            Some(changes) => Some(ScanNotice::ModsChanged {
                changes,
                removed_manual,
            }),
            None if removed_manual => Some(ScanNotice::ManualSourcesRemoved),
            None => None,
        };
    }

    pub(super) fn notice(&self) -> Option<&str> {
        self.notice.as_ref().map(ScanNotice::message)
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
            .map_or(&[][..], |settings| settings.source_mods.as_slice());
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
        if let Some(notice) = &self.notice {
            ui.colored_label(ui.visuals().warn_fg_color, notice.message())
                .on_hover_ui(|ui| {
                    ui.label(notice.to_string());
                });
        }
        ui.collapsing("User data directory", |ui| {
            ui.horizontal(|ui| {
                let mut directory = profile
                    .auto_source_mods
                    .as_ref()
                    .and_then(|settings| settings.user_data_dir.as_deref())
                    .map_or(Cow::Borrowed(""), Path::to_string_lossy);
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut directory)
                            .hint_text("Auto")
                            .desired_width(540.0),
                    )
                    .changed()
                {
                    let directory =
                        (!directory.is_empty()).then(|| PathBuf::from(directory.as_ref()));
                    profile
                        .auto_source_mods
                        .get_or_insert_default()
                        .user_data_dir = directory;
                    changed = true;
                }
                picker.button(
                    ui,
                    Target::UserData,
                    profile
                        .auto_source_mods
                        .as_ref()
                        .and_then(|settings| settings.user_data_dir.as_deref())
                        .unwrap_or_else(|| Path::new("")),
                );
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
