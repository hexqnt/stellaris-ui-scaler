use eframe::egui::{self, Vec2};

use crate::config::{Algorithm, Category};

use super::{Selection, Studio, icons, theme};

impl Studio {
    fn asset_search_field(&mut self, ui: &mut egui::Ui) -> egui::Response {
        let id = egui::Id::unique("asset-search");
        let clear_with_escape = !self.filter.is_empty()
            && ui.memory(|memory| memory.had_focus_last_frame(id))
            && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        if clear_with_escape {
            self.filter.clear();
        }
        let clear_id = egui::IdSalt::new("clear-search");
        let mut output = egui::TextEdit::singleline(&mut self.filter)
            .id(id)
            .hint_text("Search assets…")
            .prefix(egui::RichText::new(icons::SEARCH).color(theme::MUTED))
            .suffix(egui::Atom::custom(clear_id, Vec2::splat(22.0)))
            .desired_width(f32::INFINITY)
            .show(ui);
        if clear_with_escape {
            output.response.mark_changed();
            output.response.request_focus();
        }
        if !self.filter.is_empty()
            && let Some(rect) = output.response.rect(clear_id)
        {
            let clear = ui
                .put(rect, egui::Button::new(icons::CLEAR).small().frame(false))
                .on_hover_text("Clear search");
            clear.widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), "Clear search")
            });
            if clear.clicked() {
                self.filter.clear();
                output.response.mark_changed();
                output.response.request_focus();
            }
        }
        output
            .response
            .response
            .on_hover_text("Ctrl+F: search assets · Esc: clear search")
    }
    pub(super) fn assets_panel(&mut self, ui: &mut egui::Ui, busy: bool) {
        let previous_selection = self.selected;
        if ui
            .add_enabled(self.plan.is_some(), egui::Button::new("Search by image…"))
            .clicked()
        {
            self.visual_search.open = true;
        }
        let mut changed = self.asset_search_field(ui).changed();
        egui::ComboBox::from_id_salt("asset-category")
            .width(ui.available_width())
            .selected_text(self.category.map_or("All categories", Category::name))
            .show_ui(ui, |ui| {
                changed |= ui
                    .selectable_value(&mut self.category, None, "All categories")
                    .changed();
                for category in Category::ALL {
                    changed |= ui
                        .selectable_value(&mut self.category, Some(category), category.name())
                        .changed();
                }
            });
        if changed {
            self.refresh_asset_filter();
        }
        ui.columns(2, |columns| {
            let expand = &mut columns[0];
            if expand
                .add_sized(
                    [expand.available_width(), 32.0],
                    egui::Button::new("Expand all"),
                )
                .clicked()
            {
                self.asset_tree.expand_all();
            }
            let collapse = &mut columns[1];
            if collapse
                .add_sized(
                    [collapse.available_width(), 32.0],
                    egui::Button::new("Collapse all"),
                )
                .clicked()
            {
                self.asset_tree.collapse_all();
            }
        });
        ui.small(format!(
            "{} matching textures",
            self.asset_search.matches.len()
        ));
        if self.plan.is_some() && self.asset_search.matches.is_empty() {
            ui.weak("No textures match. Try another search or category.");
        }
        if let Some(category) = self.category
            && ui
                .add_enabled(!busy, egui::Button::new("Edit category rule"))
                .clicked()
        {
            self.rule_selection = Some(Selection::Category(category));
            self.selected = None;
        }
        self.draw_asset_tree(ui, busy);
        if previous_selection != self.selected {
            self.invalidate_preview();
            self.select_current_algorithm();
        }
    }
    pub(super) fn apply_selected_preview(&mut self) {
        if !self.candidate_ready(self.chosen) {
            return;
        }
        if let Some(target) = &self.rule_selection {
            target.set_algorithm(&mut self.profile, self.algorithms[self.chosen]);
            self.status = "Algorithm override updated; save the profile to persist it".into();
        }
    }

    pub(super) fn candidates_panel(&mut self, ui: &mut egui::Ui, busy: bool) {
        let previous = (self.algorithms, self.count);
        ui.add_enabled_ui(!busy, |ui| {
            ui.horizontal(|ui| {
                ui.label("Candidates");
                ui.add(egui::DragValue::new(&mut self.count).range(2..=4));
            });
            for (i, algorithm) in self.algorithms.iter_mut().take(self.count).enumerate() {
                egui::ComboBox::from_id_salt(i)
                    .selected_text(format!("{}: {}", i + 1, algorithm.name()))
                    .show_ui(ui, |ui| {
                        for option in Algorithm::ALL {
                            ui.selectable_value(algorithm, option, option.name());
                        }
                    });
            }
        });
        self.chosen = self.chosen.min(self.count - 1);
        if ui
            .add_enabled(
                self.candidate_ready(self.chosen) && self.rule_selection.is_some(),
                egui::Button::new("Use selected preview filter"),
            )
            .clicked()
        {
            self.apply_selected_preview();
        }
        if previous != (self.algorithms, self.count) {
            self.invalidate_preview();
        }
    }
}
