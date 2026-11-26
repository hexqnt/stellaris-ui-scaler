use eframe::egui::{self, Color32, Pos2, Rect, TextureHandle, Vec2};

use super::{ComparisonMode, PreviewBackground, PreviewImage, Studio, icons, theme};

impl Studio {
    pub(super) fn accept_preview(
        &mut self,
        ctx: &egui::Context,
        event: crate::compiler::PreviewEvent,
    ) {
        let upload = |name: &str, image: &image::RgbaImage| {
            ctx.load_texture(
                name,
                egui::ColorImage::from_rgba_unmultiplied(
                    [image.width() as usize, image.height() as usize],
                    image.as_raw(),
                ),
                egui::TextureOptions::NEAREST,
            )
        };
        match event {
            crate::compiler::PreviewEvent::Source { image, features } => {
                self.preview_features = features;
                self.select_current_algorithm();
                let original = upload("Original", &image);
                self.images = std::iter::once(("Original".into(), PreviewImage::Ready(original)))
                    .chain(self.algorithms[..self.count].iter().enumerate().map(
                        |(index, algorithm)| {
                            (
                                format!("{}: {}", index + 1, algorithm.name()),
                                PreviewImage::Pending,
                            )
                        },
                    ))
                    .collect();
            }
            crate::compiler::PreviewEvent::Candidate { index, image } => match image {
                Ok(image) => {
                    let (name, handle) = &mut self.images[index + 1];
                    *handle = PreviewImage::Ready(upload(name, &image));
                }
                Err(error) => {
                    self.images[index + 1].1 = PreviewImage::Failed(format!("{error:#}"));
                }
            },
        }
    }

    pub(super) fn candidate_ready(&self, index: usize) -> bool {
        self.images
            .get(index + 1)
            .is_some_and(|(_, image)| image.handle().is_some())
    }

    fn candidate_placeholder(&self, ui: &mut egui::Ui, index: usize) {
        match &self.images[index + 1].1 {
            PreviewImage::Pending => {
                ui.spinner();
                ui.label("Preparing preview…");
            }
            PreviewImage::Failed(error) => {
                ui.colored_label(theme::MUTED, error);
            }
            PreviewImage::Ready(_) => {}
        }
    }

    pub(super) fn preview_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            for (mode, icon, label) in [
                (ComparisonMode::Compare, icons::COMPARE, "Compare"),
                (ComparisonMode::Split, icons::SPLIT, "Split"),
                (ComparisonMode::Ab, icons::AB, "A/B"),
            ] {
                if ui
                    .add(icons::button(icon, label).selected(self.mode == mode))
                    .clicked()
                {
                    self.mode = mode;
                    self.preview_offset = Vec2::ZERO;
                }
            }
            ui.separator();
            if ui
                .add(icons::button(icons::ZOOM_OUT, ""))
                .on_hover_text("Zoom out")
                .clicked()
            {
                self.zoom = (self.zoom / 2.0).max(0.25);
                self.fit_preview = false;
            }
            let mut percent = self.zoom * 100.0;
            if ui
                .add(
                    egui::DragValue::new(&mut percent)
                        .range(25.0..=3200.0)
                        .suffix("%")
                        .speed(10.0),
                )
                .changed()
            {
                self.zoom = percent / 100.0;
                self.fit_preview = false;
            }
            if ui
                .add(icons::button(icons::ZOOM_IN, ""))
                .on_hover_text("Zoom in")
                .clicked()
            {
                self.zoom = (self.zoom * 2.0).min(32.0);
                self.fit_preview = false;
            }
            if ui
                .add(icons::button(icons::FIT, "Fit").selected(self.fit_preview))
                .clicked()
            {
                self.fit_preview.toggle();
                self.preview_offset = Vec2::ZERO;
            }
            egui::ComboBox::from_id_salt("preview-background")
                .width(90.0)
                .selected_text(match self.background {
                    PreviewBackground::Checker => "Checker",
                    PreviewBackground::Dark => "Dark",
                    PreviewBackground::Light => "Light",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.background,
                        PreviewBackground::Checker,
                        "Checker",
                    );
                    ui.selectable_value(&mut self.background, PreviewBackground::Dark, "Dark");
                    ui.selectable_value(&mut self.background, PreviewBackground::Light, "Light");
                });
        });
    }

    fn candidate_navigation(&mut self, ui: &mut egui::Ui) {
        let count = self.images.len().saturating_sub(1);
        if count == 0 {
            return;
        }
        self.chosen = self.chosen.min(count - 1);
        let (mut previous, mut next) = if ui.ctx().text_edit_focused() {
            (false, false)
        } else {
            ui.input_mut(|input| {
                (
                    input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft),
                    input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight),
                )
            })
        };
        ui.horizontal_wrapped(|ui| {
            previous |= ui
                .add(icons::button(icons::PREVIOUS, ""))
                .on_hover_text("Previous candidate (Left)")
                .clicked();
            next |= ui
                .add(icons::button(icons::NEXT, ""))
                .on_hover_text("Next candidate (Right)")
                .clicked();
            if previous {
                self.chosen = (self.chosen + count - 1) % count;
            }
            if next {
                self.chosen = (self.chosen + 1) % count;
            }
            ui.weak(format!("{} / {count}", self.chosen + 1));
            ui.label(&self.images[self.chosen + 1].0);
            if self.mode == ComparisonMode::Split {
                ui.weak("Drag the divider to compare with the original");
            }
        });
        if previous || next {
            self.apply_selected_preview();
            ui.ctx().request_repaint();
        }
    }

    pub(super) fn draw_image(
        &self,
        ui: &mut egui::Ui,
        texture: &TextureHandle,
        overlay: Option<&TextureHandle>,
        viewport: Vec2,
    ) -> (egui::Response, Option<f32>) {
        let native_size = overlay.unwrap_or(texture).size_vec2() / ui.ctx().pixels_per_point();
        let zoom = if self.fit_preview {
            let available = (viewport - Vec2::splat(40.0)).max(Vec2::splat(1.0));
            (available.x / native_size.x).min(available.y / native_size.y)
        } else {
            self.zoom
        };
        let image_size = native_size * zoom;
        let canvas_size = image_size.max(viewport);
        let (canvas, response) = ui.allocate_exact_size(canvas_size, egui::Sense::click_and_drag());
        let response = image_interaction(ui, response, overlay.is_some());
        let painter = ui.painter();
        if self.background == PreviewBackground::Checker {
            let cell = 16.0;
            let visible = painter.clip_rect().intersect(canvas);
            let start_x = ((visible.left() - canvas.left()) / cell).max(0.0) as usize;
            let start_y = ((visible.top() - canvas.top()) / cell).max(0.0) as usize;
            for y in start_y..((visible.bottom() - canvas.top()) / cell).ceil().max(0.0) as usize {
                for x in
                    start_x..((visible.right() - canvas.left()) / cell).ceil().max(0.0) as usize
                {
                    let start = canvas.min + Vec2::new(x as f32 * cell, y as f32 * cell);
                    painter.rect_filled(
                        Rect::from_min_size(start, Vec2::splat(cell)).intersect(canvas),
                        0.0,
                        if (x + y).is_multiple_of(2) {
                            Color32::from_rgb(43, 48, 51)
                        } else {
                            Color32::from_rgb(54, 59, 62)
                        },
                    );
                }
            }
        } else {
            painter.rect_filled(
                canvas,
                0.0,
                if self.background == PreviewBackground::Dark {
                    Color32::BLACK
                } else {
                    Color32::WHITE
                },
            );
        }
        let rect = Rect::from_center_size(canvas.center(), image_size);
        painter.image(
            texture.id(),
            rect,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );
        if let Some(overlay) = overlay {
            let split = if response.dragged_by(egui::PointerButton::Primary) || response.clicked() {
                response
                    .interact_pointer_pos()
                    .map(|pos| ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0))
            } else {
                None
            };
            let fraction = split.unwrap_or(self.split);
            let left = rect.width().mul_add(fraction, rect.left());
            let target = Rect::from_min_max(Pos2::new(left, rect.top()), rect.max);
            painter.image(
                overlay.id(),
                target,
                Rect::from_min_max(Pos2::new(fraction, 0.0), Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
            painter.line_segment(
                [
                    Pos2::new(left, canvas.top()),
                    Pos2::new(left, canvas.bottom()),
                ],
                egui::Stroke::new(2.0, theme::MINT),
            );
            let handle = Pos2::new(left, canvas.center().y);
            painter.circle_filled(handle, 16.0, theme::MINT);
            painter.text(
                handle,
                egui::Align2::CENTER_CENTER,
                icons::SPLIT,
                egui::FontId::proportional(20.0),
                theme::INK,
            );
            return (response, split);
        }
        (response, None)
    }

    pub(super) fn previews_panel(&mut self, ui: &mut egui::Ui) {
        if self.images.len() < 2 {
            ui.centered_and_justified(|ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space((ui.available_height() / 2.0 - 80.0).max(0.0));
                    ui.label(
                        egui::RichText::new(icons::TEXTURES)
                            .size(42.0)
                            .color(theme::MUTED),
                    );
                    ui.heading(if self.job.is_some() {
                        "Preparing preview…"
                    } else {
                        "Your texture workspace"
                    });
                    ui.weak(if self.plan.is_some() {
                        "Select a texture in the asset browser to compare filters."
                    } else {
                        "Choose a game installation and scan its assets to begin."
                    });
                });
            });
            return;
        }
        if self.mode == ComparisonMode::Compare {
            self.comparison_grid(ui);
        } else {
            self.single_preview(ui);
        }
    }

    fn single_preview(&mut self, ui: &mut egui::Ui) {
        self.candidate_navigation(ui);
        if !self.candidate_ready(self.chosen) {
            self.candidate_placeholder(ui, self.chosen);
            return;
        }
        let Some(original) = self.images[0].1.handle() else {
            return;
        };
        let candidate = &self.images[self.chosen + 1];
        let Some(image) = candidate.1.handle() else {
            return;
        };
        ui.horizontal(|ui| {
            ui.strong(if self.mode == ComparisonMode::Split {
                "Original"
            } else {
                &candidate.0
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if self.mode == ComparisonMode::Split {
                    ui.strong(&candidate.0);
                }
            });
        });
        let viewport = ui.available_size().max(Vec2::splat(1.0));
        let output = egui::ScrollArea::both()
            .id_salt("single-preview")
            .scroll_offset(self.preview_offset)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if self.mode == ComparisonMode::Split {
                    self.draw_image(ui, original, Some(image), viewport)
                } else {
                    self.draw_image(ui, image, None, viewport)
                }
            });
        if let Some(split) = output.inner.1 {
            self.split = split;
        }
        self.preview_offset = output.state.clamped_offset();
    }

    fn comparison_grid(&mut self, ui: &mut egui::Ui) {
        let Some(original) = self.images[0].1.handle() else {
            return;
        };
        ui.collapsing("Original texture", |ui| {
            let height = 100.0;
            let viewport = Vec2::new(ui.available_width(), height);
            egui::ScrollArea::both()
                .id_salt("original-preview")
                .max_height(height)
                .show(ui, |ui| {
                    self.draw_image(ui, original, None, viewport);
                });
        });
        let selected = self.current_algorithm_candidate();
        let candidates = &self.images[1..];
        let columns = candidates.len().min(2);
        let rows = candidates.len().div_ceil(2);
        let row_height = (ui
            .spacing()
            .item_spacing
            .y
            .mul_add(-((rows - 1) as f32), ui.available_height())
            / rows as f32)
            .max(100.0);
        let mut next_chosen = None;
        let mut next_offset = self.preview_offset;
        for (row, images) in candidates.chunks(2).enumerate() {
            ui.columns(columns, |cells| {
                for (column, ((name, image), cell)) in images.iter().zip(cells).enumerate() {
                    let index = row * 2 + column;
                    egui::Frame::new()
                        .inner_margin(8)
                        .stroke(egui::Stroke::new(
                            1.0,
                            if selected == Some(index) {
                                theme::MINT
                            } else {
                                theme::BORDER
                            },
                        ))
                        .corner_radius(5)
                        .show(cell, |ui| {
                            ui.set_min_width(ui.available_width());
                            if ui
                                .add_enabled(
                                    self.candidate_ready(index),
                                    egui::Button::selectable(selected == Some(index), name),
                                )
                                .clicked()
                            {
                                next_chosen = Some(index);
                            }
                            let Some(image) = image.handle() else {
                                ui.set_min_height((row_height - 40.0).max(60.0));
                                self.candidate_placeholder(ui, index);
                                return;
                            };
                            let viewport =
                                Vec2::new(ui.available_width(), (row_height - 60.0).max(40.0));
                            let output = egui::ScrollArea::both()
                                .id_salt(("candidate-preview", index))
                                .scroll_offset(self.preview_offset)
                                .auto_shrink([false, false])
                                .max_height(viewport.y)
                                .show(ui, |ui| self.draw_image(ui, image, None, viewport).0);
                            if output.inner.clicked() {
                                next_chosen = Some(index);
                            }
                            if ui.rect_contains_pointer(output.inner_rect)
                                || output.inner.dragged_by(egui::PointerButton::Middle)
                            {
                                next_offset = output.state.clamped_offset();
                            }
                        });
                }
            });
        }
        if self.preview_offset != next_offset {
            self.preview_offset = next_offset;
            ui.ctx().request_repaint();
        }
        if let Some(index) = next_chosen {
            self.chosen = index;
            self.apply_selected_preview();
            ui.ctx().request_repaint();
        }
    }
}
fn image_interaction(ui: &egui::Ui, response: egui::Response, split: bool) -> egui::Response {
    let cursor = if response.dragged_by(egui::PointerButton::Middle) {
        ui.scroll_with_delta_animation(response.drag_delta(), egui::style::ScrollAnimation::none());
        egui::CursorIcon::Grabbing
    } else if split {
        egui::CursorIcon::ResizeHorizontal
    } else {
        egui::CursorIcon::PointingHand
    };
    response.on_hover_and_drag_cursor(cursor)
}
