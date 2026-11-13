use eframe::egui;

use crate::config::{Algorithm, ColorSpace, Scale};

use super::{icons, theme};

pub(super) fn scale(ui: &mut egui::Ui, value: &mut Scale, suffix: &str) -> bool {
    let mut raw = value.get();
    let control = egui::DragValue::new(&mut raw)
        .range(0.25..=8.0)
        .speed(0.01)
        .suffix(suffix);
    if ui.add(control).changed()
        && let Ok(scale) = Scale::try_from(raw)
    {
        *value = scale;
        return true;
    }
    false
}

pub(super) fn algorithm(
    ui: &mut egui::Ui,
    id: impl egui::AsIdSalt,
    width: f32,
    value: &mut Algorithm,
) {
    egui::ComboBox::from_id_salt(id)
        .width(width)
        .selected_text(value.name())
        .show_ui(ui, |ui| {
            for option in Algorithm::ALL {
                ui.selectable_value(value, option, option.name());
            }
        });
}

pub(super) fn color_space(
    ui: &mut egui::Ui,
    id: impl egui::AsIdSalt,
    width: f32,
    value: &mut ColorSpace,
) {
    egui::ComboBox::from_id_salt(id)
        .width(width)
        .selected_text(match value {
            ColorSpace::Srgb => "sRGB",
            ColorSpace::Linear => "Linear RGB",
        })
        .show_ui(ui, |ui| {
            ui.selectable_value(value, ColorSpace::Srgb, "sRGB");
            ui.selectable_value(value, ColorSpace::Linear, "Linear RGB");
        });
}

pub(super) fn field<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut Option<T>,
    inherited: T,
    edit: impl FnOnce(&mut egui::Ui, &mut T),
) {
    ui.push_id(label, |ui| {
        ui.horizontal(|ui| {
            let local = value.is_some();
            let label_width = (ui.available_width() - 146.0).clamp(80.0, 130.0);
            ui.allocate_ui_with_layout(
                egui::vec2(label_width, 32.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.set_min_width(label_width);
                    let text = egui::RichText::new(label).color(if local {
                        theme::MINT
                    } else {
                        ui.visuals().text_color()
                    });
                    ui.add(egui::Label::new(text).wrap())
                        .on_hover_text(if local {
                            "Local override. Reset to use the inherited value."
                        } else {
                            "Inherited value. Editing creates an override for this selection."
                        });
                },
            );
            let previous = value.unwrap_or(inherited);
            let mut displayed = previous;
            ui.allocate_ui_with_layout(
                egui::vec2(110.0, 32.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.set_min_width(110.0);
                    edit(ui, &mut displayed);
                },
            );
            if displayed != previous {
                *value = Some(displayed);
            }
            if value.is_some() {
                let reset = ui
                    .add_sized(
                        [20.0, 24.0],
                        egui::Button::new(icons::RESET).small().frame(false),
                    )
                    .on_hover_text("Reset to inherited value");
                reset.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::Role::Button,
                        ui.is_enabled(),
                        format!("Reset {label}"),
                    )
                });
                if reset.clicked() {
                    *value = None;
                }
            } else {
                ui.allocate_space(egui::vec2(20.0, 24.0));
            }
        });
    });
}
