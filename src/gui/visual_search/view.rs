use std::{
    path::Path,
    sync::{Arc, atomic::Ordering},
};

use super::super::path_picker::Kind;

use eframe::egui;

use super::{Input, VisualSearch, worker::Progress};
use crate::{compiler::BuildPlan, config::Profile};

const ROW_HEIGHT: f32 = 80.0;

#[derive(Default)]
pub(super) struct ListResponse {
    pub selected: Option<usize>,
    pub load_more: bool,
}

impl VisualSearch {
    pub(super) fn controls(&mut self, ui: &mut egui::Ui) -> Option<Input> {
        let mut input = None;
        ui.label("Use a tightly cropped screenshot of an icon or button.");
        ui.weak(
            "Ranking: dimensions, colors, then shape. Source and profile scales are considered.",
        );
        ui.label("Drop an image here, paste it, or enter its file path.");
        ui.horizontal(|ui| {
            let path = egui::TextEdit::singleline(&mut self.path)
                .hint_text("Image file path")
                .desired_width(380.0);
            ui.add(path);
            if ui.button("Browse…").clicked() {
                self.picker.open(
                    (),
                    Kind::Image,
                    Path::new(self.path.trim()),
                    "Select search image",
                );
            }
            if ui.button("Load image").clicked() {
                input = Some(Input::File(self.path.trim().into()));
            }
            if ui.button("Paste image").clicked() {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            }
        });
        // События потребляются один раз, в том числе при повторном проходе компоновки egui.
        ui.input_mut(|state| {
            if let Some(position) = state
                .events
                .iter()
                .position(|event| matches!(event, egui::Event::PasteImage(_)))
                && let egui::Event::PasteImage(image) = state.events.remove(position)
            {
                input = Some(Input::Image(image));
            }
            if let Some(file) = state.raw.dropped_files.drain(..).next() {
                input = Some(Input::File(file.path().to_owned()));
            }
        });
        input
    }

    fn progress(&mut self, ui: &mut egui::Ui, total: usize) {
        let mut cancel = false;
        if let Some(job) = &self.job {
            ui.horizontal(|ui| {
                ui.spinner();
                match &job.progress {
                    Progress::Indexing(completed) => {
                        let completed = completed.load(Ordering::Relaxed);
                        ui.label(format!("Processed {completed}/{total} textures"));
                    }
                    Progress::Searching => {
                        ui.label("Finding similar textures…");
                    }
                    Progress::Page => {
                        ui.label("Loading more results…");
                    }
                }
                cancel = ui.button("Cancel search").clicked();
            });
        }
        if cancel {
            self.cancel();
        }
        ui.label(&self.status);
    }

    pub(super) fn results(
        &self,
        ui: &mut egui::Ui,
        plan: &BuildPlan,
        selected: Option<usize>,
        busy: bool,
    ) -> ListResponse {
        let mut clicked = None;
        let output = egui::ScrollArea::vertical()
            .auto_shrink([false, true])
            .id_salt(("visual-search-results", self.generation))
            .max_height(480.0)
            .show_rows(ui, ROW_HEIGHT, self.rows.len(), |ui, range| {
                for rank in range {
                    let row = &self.rows[rank];
                    let hit = &row.hit;
                    let asset = &plan.textures[hit.asset];
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), ROW_HEIGHT),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.image(&row.texture);
                            ui.vertical(|ui| {
                                let label = format!("{}. {}", rank + 1, asset.path);
                                let button =
                                    egui::Button::selectable(selected == Some(hit.asset), label)
                                        .truncate();
                                if ui
                                    .add_enabled(!busy, button)
                                    .on_hover_text(&asset.path)
                                    .clicked()
                                {
                                    clicked = Some(hit.asset);
                                }
                                let r = hit.region;
                                ui.small(format!(
                                    "Frame {} · {}×{} · distance {:.3} (lower is closer)",
                                    hit.frame + 1,
                                    r.width,
                                    r.height,
                                    hit.distance
                                ));
                            });
                        },
                    );
                }
            });
        let more = self.rows.len() < self.ranked.len();
        let near_bottom = output.state.clamped_offset().y + output.inner_rect.height()
            >= ROW_HEIGHT.mul_add(-2.0, output.content_size.y);
        // После отмены или ошибки новая прокрутка позволяет повторить загрузку без цикла автоповторов.
        let resume = self.paging_paused
            && ui.rect_contains_pointer(output.inner_rect)
            && ui.input(|input| {
                input.events.iter().any(
                    |event| matches!(event, egui::Event::MouseWheel { delta, .. } if delta.y < 0.0),
                )
            });
        ListResponse {
            selected: clicked,
            load_more: near_bottom && more && self.job.is_none() && (!self.paging_paused || resume),
        }
    }

    pub(super) fn show(
        &mut self,
        ctx: &egui::Context,
        plan: &Arc<BuildPlan>,
        profile: &Profile,
        selected: Option<usize>,
        busy: bool,
    ) -> Option<usize> {
        if !self.open {
            return None;
        }
        let mut open = true;
        let mut input = None;
        let mut response = ListResponse::default();
        egui::Window::new("Search by image")
            .open(&mut open)
            .default_size([640.0, 620.0])
            .show(ctx, |ui| {
                input = self.controls(ui);
                if let Some(query) = &self.query {
                    ui.image(query);
                }
                self.progress(ui, plan.textures.len());
                ui.separator();
                response = self.results(ui, plan, selected, busy);
            });
        if let Some(((), path)) = self.picker.show(ctx) {
            self.path = path.display().to_string();
            input = Some(Input::File(path));
        }
        self.open = open && response.selected.is_none();
        if !self.open {
            self.close();
        } else if let Some(input) = input {
            self.start(ctx, Arc::clone(plan), profile.clone(), input);
        } else if response.load_more {
            self.load_next_page(ctx, Arc::clone(plan));
        }
        response.selected
    }
}
