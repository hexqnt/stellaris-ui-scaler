use std::{
    path::Path,
    sync::{Arc, atomic::Ordering},
};

use eframe::egui;

use crate::{
    compiler::{BuildPlan, TextureAsset},
    config::Profile,
};

use super::super::{path_picker::Kind, theme};

use super::{Input, ResultRow, VisualSearch, worker::Progress};

const ROW_HEIGHT: f32 = 112.0;

struct Columns {
    rank: egui::Rect,
    preview: egui::Rect,
    resource: egui::Rect,
    distance: egui::Rect,
}

impl Columns {
    fn new(rect: egui::Rect) -> Self {
        let inner = rect.shrink2(egui::vec2(10.0, 0.0));
        let rank = egui::Rect::from_min_size(inner.min, egui::vec2(28.0, inner.height()));
        let preview = egui::Rect::from_min_size(
            rank.right_top() + egui::vec2(10.0, 0.0),
            egui::vec2(88.0, inner.height()),
        );
        let distance =
            egui::Rect::from_min_max(egui::pos2(inner.right() - 82.0, inner.top()), inner.max);
        let resource = egui::Rect::from_min_max(
            preview.right_top() + egui::vec2(14.0, 0.0),
            distance.left_bottom() - egui::vec2(14.0, 0.0),
        );
        Self {
            rank,
            preview,
            resource,
            distance,
        }
    }
}

#[derive(Default)]
pub(super) struct ListResponse {
    pub selected: Option<usize>,
    pub load_more: bool,
}

impl ResultRow {
    fn show(
        &self,
        ui: &mut egui::Ui,
        rank: usize,
        asset: &TextureAsset,
        profile: &Profile,
        selected: bool,
        busy: bool,
    ) -> bool {
        let enabled = ui.is_enabled() && !busy;
        let sense = if enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        };
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), ROW_HEIGHT), sense);
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::Role::Button, enabled, selected, &asset.path)
        });
        let highlighted = enabled && (response.hovered() || response.has_focus());
        let fill = if highlighted {
            theme::BORDER
        } else {
            theme::SURFACE
        };
        ui.painter().rect(
            rect,
            5.0,
            fill,
            egui::Stroke::new(1.0, if selected { theme::MINT } else { theme::BORDER }),
            egui::StrokeKind::Inside,
        );
        let columns = Columns::new(rect);
        ui.painter().text(
            columns.rank.center(),
            egui::Align2::CENTER_CENTER,
            format!("{}", rank + 1),
            egui::FontId::monospace(12.0),
            theme::MUTED,
        );

        let tile = egui::Rect::from_center_size(
            columns.preview.center(),
            egui::vec2(columns.preview.width(), 72.0),
        );
        ui.painter().rect_filled(tile, 4.0, theme::BACKGROUND);
        let size = self.texture.size_vec2();
        let available = tile.size() - egui::Vec2::splat(8.0);
        let scale = (available.x / size.x).min(available.y / size.y);
        egui::Image::new(&self.texture).paint_at(
            ui,
            egui::Rect::from_center_size(tile.center(), size * scale),
        );

        let (folder, name) = asset.path.rsplit_once('/').unwrap_or(("", &asset.path));
        let text_rect = columns.resource.shrink2(egui::vec2(0.0, 12.0));
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(text_rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
            |ui| {
                ui.spacing_mut().item_spacing.y = 5.0;
                ui.add(egui::Label::new(egui::RichText::new(name).strong()).truncate());
                ui.add(
                    egui::Label::new(egui::RichText::new(folder).small().color(theme::MUTED))
                        .truncate(),
                );
                let region = self.hit.region;
                let metadata = format!(
                    "{}  ·  {} × {} px  ·  Frame {}/{}",
                    asset.category.name(),
                    region.width,
                    region.height,
                    self.hit.frame + 1,
                    asset.frames
                );
                ui.add(
                    egui::Label::new(egui::RichText::new(metadata).small().color(theme::MUTED))
                        .truncate(),
                );
                let processing = asset.processing(profile, Some(&self.features));
                let algorithm = format!("Algorithm: {}", processing.options.algorithm.name());
                let response = ui.add(
                    egui::Label::new(egui::RichText::new(algorithm).small().color(theme::MINT))
                        .truncate(),
                );
                if let Some(choice) = processing.recommendation {
                    response.on_hover_text(choice.reason);
                }
            },
        );
        ui.painter().text(
            columns.distance.center(),
            egui::Align2::CENTER_CENTER,
            format!("{:.3}", self.hit.distance),
            egui::FontId::monospace(15.0),
            theme::MINT,
        );
        let clicked = response.clicked();
        let response = response.on_hover_text(&asset.path);
        if enabled {
            response.on_hover_cursor(egui::CursorIcon::PointingHand);
        }
        clicked
    }
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
        profile: &Profile,
        selected: Option<usize>,
        busy: bool,
    ) -> ListResponse {
        let mut clicked = None;
        let (header, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 20.0), egui::Sense::hover());
        let columns = Columns::new(header);
        let font = egui::FontId::proportional(12.0);
        for (position, alignment, text) in [
            (columns.rank.center(), egui::Align2::CENTER_CENTER, "#"),
            (
                columns.preview.center(),
                egui::Align2::CENTER_CENTER,
                "Preview",
            ),
            (
                columns.resource.left_center(),
                egui::Align2::LEFT_CENTER,
                "Resource",
            ),
            (
                columns.distance.center(),
                egui::Align2::CENTER_CENTER,
                "Distance ↓",
            ),
        ] {
            ui.painter()
                .text(position, alignment, text, font.clone(), theme::MUTED);
        }
        ui.interact(
            columns.distance,
            ui.scope_id().with("distance-heading"),
            egui::Sense::hover(),
        )
        .on_hover_text("Lower distance means a closer match in size, colors and shape");
        let output = egui::ScrollArea::vertical()
            .auto_shrink([false, true])
            .id_salt(("visual-search-results", self.generation))
            .max_height(480.0)
            .show_rows(ui, ROW_HEIGHT, self.rows.len(), |ui, range| {
                for rank in range {
                    let row = &self.rows[rank];
                    let asset = &plan.textures[row.hit.asset];
                    if row.show(
                        ui,
                        rank,
                        asset,
                        profile,
                        selected == Some(row.hit.asset),
                        busy,
                    ) {
                        clicked = Some(row.hit.asset);
                    }
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
                response = self.results(ui, plan, profile, selected, busy);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::Category,
        gui::{
            tests::{action, node},
            visual_search::{descriptor::Region, index::Hit},
        },
    };

    #[test]
    fn cards_fit_long_paths_and_support_accessible_selection() {
        for width in [480.0, 640.0, 960.0] {
            let ctx = egui::Context::default();
            ctx.enable_accesskit();
            theme::install(&ctx);
            let asset = TextureAsset {
                path: format!(
                    "gfx/interface/{}/{}.dds",
                    "long_folder/".repeat(12),
                    "long_name".repeat(30)
                ),
                width: 128,
                height: 32,
                frames: 4,
                category: Category::Icon,
                tiled: false,
            };
            let row = ResultRow {
                hit: Hit {
                    asset: 0,
                    frame: 1,
                    region: Region {
                        x: 32,
                        y: 0,
                        width: 32,
                        height: 32,
                    },
                    distance: 0.149,
                },
                features: crate::config::ImageFeatures::analyze(
                    &image::RgbaImage::from_pixel(128, 32, image::Rgba([255, 255, 255, 255])),
                    4,
                )
                .unwrap(),
                texture: ctx.load_texture(
                    "test-icon",
                    egui::ColorImage::filled([32, 32], egui::Color32::WHITE),
                    egui::TextureOptions::NEAREST,
                ),
            };
            let mut profile = Profile::default();
            profile.assets.insert(
                asset.path.clone(),
                crate::config::TextureRule {
                    algorithm: Some(crate::config::Algorithm::Nearest),
                    ..Default::default()
                },
            );
            let mut selected = false;
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 400.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    let available = ui.available_rect_before_wrap();
                    selected |= row.show(ui, 0, &asset, &profile, false, false);
                    assert!(ui.min_rect().right() <= available.right() + 1.0);
                    assert!(ui.min_rect().height() <= ROW_HEIGHT + 1.0);
                },
            );
            assert!(!selected);
            let button = node(&output, |node| {
                node.role() == egui::Role::Button && node.label() == Some(asset.path.as_str())
            });
            assert!(output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text().contains("Frame 2/4"))));
            assert!(output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "Algorithm: Nearest")));
            output.drop_without_applying_deltas();

            profile.assets.get_mut(&asset.path).unwrap().algorithm =
                Some(crate::config::Algorithm::Lanczos3);
            let output = ctx.run_ui(
                egui::RawInput {
                    events: vec![action(egui::accesskit::Action::Click, button, None)],
                    ..Default::default()
                },
                |ui| {
                    selected |= row.show(ui, 0, &asset, &profile, false, false);
                },
            );
            assert!(output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "Algorithm: Lanczos3")));
            output.drop_without_applying_deltas();
            assert!(selected);
        }
    }
}
