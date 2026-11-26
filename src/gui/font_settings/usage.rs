use eframe::egui;

use crate::fonts::{Group, UsageArea};

pub(super) fn hint(ui: &mut egui::Ui) {
    ui.small("Areas are recognized from GUI files and widget names; typical roles describe the base game. Labels marked inferred rely on built-in group names. References use the game and enabled mods; engine defaults and runtime assignments are not included.");
}

pub(super) fn tooltip(ui: &mut egui::Ui, group: &Group) {
    ui.strong(&group.name);
    if let Some(note) = group.usage_note() {
        ui.label(note);
    }
    if group.usage_areas().next().is_some() {
        summary(ui, group, 8);
    } else {
        list(ui, group, 6);
    }
}

pub(super) fn matches_area(group: &Group, area: Option<UsageArea>) -> bool {
    area.is_none_or(|area| group.uses_area(area))
}

pub(super) fn summary(ui: &mut egui::Ui, group: &Group, limit: usize) {
    let mut count = 0;
    let mut text = String::new();
    for area in group.usage_areas() {
        count += 1;
        if count <= limit {
            if !text.is_empty() {
                text.push_str(", ");
            }
            text.push_str(area.label());
            if !group.usages.iter().any(|usage| usage.area == Some(area)) {
                text.push_str(" (inferred)");
            }
        }
    }
    if !text.is_empty() {
        ui.add(egui::Label::new(text).wrap());
    }
    if count > limit {
        ui.weak(format!("{} more areas in Where used.", count - limit));
    }
}

pub(super) fn list(ui: &mut egui::Ui, group: &Group, limit: usize) {
    if group.usages.is_empty() {
        ui.weak("No explicit GUI references found. This does not mean the font is unused.");
        return;
    }
    let mut file = None;
    for usage in group.usages.iter().take(limit) {
        if file != Some(usage.file.as_ref()) {
            ui.add_space(4.0);
            ui.strong(usage.file.as_ref());
            file = Some(usage.file.as_ref());
        }
        ui.label(format!(
            "Line {} · {} · {} · {}",
            usage.line,
            usage.widget.as_deref().unwrap_or("Unnamed widget"),
            usage.kind,
            usage.property,
        ));
    }
    if group.usages.len() > limit {
        ui.weak(format!(
            "{} more references. Select the group and expand Where used to see all.",
            group.usages.len() - limit
        ));
    }
}
