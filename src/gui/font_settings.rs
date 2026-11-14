use eframe::egui::{self, RichText};

use crate::{
    config::{FontMode, FontRule, FontSize, Profile, Scale},
    fonts::{self, Catalog, Group, UsageArea},
};

use super::{icons, settings, theme};

pub(super) struct FontSettings {
    language: String,
    selected: usize,
    advanced: bool,
    area: Option<UsageArea>,
}

impl FontSettings {
    pub(super) fn for_profile(profile: &Profile) -> Self {
        Self {
            advanced: profile
                .fonts
                .values()
                .any(|rule| *rule != FontRule::default()),
            ..Self::default()
        }
    }

    pub(super) fn show(&mut self, ui: &mut egui::Ui, catalog: &Catalog, profile: &mut Profile) {
        if !catalog.languages.is_empty() && !catalog.languages.contains(&self.language) {
            self.language.clone_from(&catalog.languages[0]);
        }
        egui::Panel::top("font-defaults")
            .resizable(false)
            .frame(theme::frame(theme::PANEL, 0))
            .show(ui, |ui| {
                ui.heading("Fonts");
                ui.weak("Choose a font style and text size, then build the mod.");
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    font_style(ui, profile);
                    ui.separator();
                    default_scale(ui, profile);
                });
                ui.checkbox(&mut self.advanced, "Advanced font settings");
                if self.advanced {
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Preview language");
                        egui::ComboBox::from_id_salt("font-language").width(150.0)
                            .selected_text(language_name(&self.language))
                            .show_ui(ui, |ui| {
                                for language in &catalog.languages {
                                    if ui.selectable_label(self.language == *language, language_name(language)).clicked() {
                                        self.language.clone_from(language);
                                    }
                                }
                            });
                    });
                    ui.small("Group settings apply to all languages. Preview language changes only the displayed values.");
                }
                ui.add_space(12.0);
            });
        if self.advanced {
            self.group_editor(ui, catalog, profile);
        } else {
            Self::overview(ui, catalog, profile);
        }
    }

    fn overview(ui: &mut egui::Ui, catalog: &Catalog, profile: &Profile) {
        egui::CentralPanel::default().frame(theme::frame(theme::PANEL, 20)).show(ui, |ui| {
            ui.set_max_width(680.0);
            ui.heading(mode_name(profile.font_mode));
            ui.label(match profile.font_mode {
                FontMode::Automatic => "Follow the existing language rules and game/mod font assignments.",
                FontMode::TrueType => "Use sharp TrueType text for supported interface fonts, including Russian. TrueType fonts supplied by mods are kept. Fonts without a suitable replacement keep their original typeface.",
                FontMode::Original => "Preserve the original typefaces supplied by the game and mods, at the selected text size.",
            });
            ui.add_space(16.0);
            if catalog.groups.is_empty() {
                ui.weak("Scan assets to load fonts from the game and enabled mods.");
            } else {
                ui.label("Fonts from the game and enabled mods are ready.");
                ui.small("Advanced font settings lets you inspect or customize individual groups.");
            }
            ui.add_space(16.0);
            ui.label("Build mod applies your choice. Save profile keeps it for future builds.");
        });
    }

    fn group_editor(&mut self, ui: &mut egui::Ui, catalog: &Catalog, profile: &mut Profile) {
        if catalog.groups.is_empty() {
            ui.label("No font groups found in this installation.");
            return;
        }
        self.selected = self.selected.min(catalog.groups.len() - 1);
        let scale = profile.font_scale.unwrap_or(profile.scale);
        let mode = profile.font_mode;
        egui::Panel::left("font-groups")
            .default_size(270.0)
            .size_range(210.0..=360.0)
            .frame(theme::frame(theme::PANEL, 12))
            .show(ui, |ui| {
                ui.strong("Font groups");
                egui::ComboBox::from_id_salt("font-usage-area")
                    .width(ui.available_width())
                    .selected_text(self.area.map_or("All interface areas", UsageArea::label))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.area, None, "All interface areas");
                        for &area in UsageArea::ALL {
                            ui.selectable_value(&mut self.area, Some(area), area.label());
                        }
                    });
                if !matches_area(&catalog.groups[self.selected], self.area)
                    && let Some(index) = catalog
                        .groups
                        .iter()
                        .position(|group| matches_area(group, self.area))
                {
                    self.selected = index;
                }
                ui.small("Hover over a group to see where it is used.");
                ui.horizontal(|ui| {
                    ui.weak("Group");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.small("Source / result");
                    });
                });
                egui::ScrollArea::vertical()
                    .id_salt("font-group-list")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 3.0;
                        for (index, group) in catalog.groups.iter().enumerate() {
                            if !matches_area(group, self.area) {
                                continue;
                            }
                            let default = FontRule::default();
                            let rule = profile.fonts.get(&group.name).unwrap_or(&default);
                            let result =
                                group.resolve(&self.language, rule, scale, profile.font_mode);
                            let original = group.original(&self.language);
                            let row = egui::Button::selectable(self.selected == index, &group.name)
                                .right_text((
                                    size_text(original.size),
                                    icons::NEXT,
                                    size_text(result.size),
                                ))
                                .truncate();
                            if ui
                                .add_sized([ui.available_width(), 34.0], row)
                                .on_hover_ui(|ui| usage_tooltip(ui, group))
                                .clicked()
                            {
                                self.selected = index;
                            }
                        }
                    });
            });
        egui::CentralPanel::default()
            .frame(theme::frame(theme::PANEL, 20))
            .show(ui, |ui| {
                if !matches_area(&catalog.groups[self.selected], self.area) {
                    ui.weak("No fonts found for this area in the scanned GUI files. Try All interface areas.");
                    return;
                }
                egui::ScrollArea::vertical()
                    .id_salt("font-group-editor")
                    .show(ui, |ui| {
                        ui.set_max_width(ui.available_width().min(680.0));
                        let group = &catalog.groups[self.selected];
                        let mut new_rule = FontRule::default();
                        let rule = profile.fonts.get_mut(&group.name).unwrap_or(&mut new_rule);
                        ui.push_id(&group.name, |ui| {
                            group_controls(ui, catalog, group, &self.language, scale, rule, mode);
                        });
                        if *rule == FontRule::default() {
                            profile.fonts.remove(&group.name);
                        } else if new_rule != FontRule::default() {
                            profile.fonts.insert(group.name.clone(), new_rule);
                        }
                    });
            });
    }
}

impl Default for FontSettings {
    fn default() -> Self {
        Self {
            language: "l_russian".into(),
            selected: 0,
            advanced: false,
            area: None,
        }
    }
}

const fn mode_name(mode: FontMode) -> &'static str {
    match mode {
        FontMode::Automatic => "Automatic",
        FontMode::TrueType => "Sharp text (TrueType)",
        FontMode::Original => "Original fonts",
    }
}

fn font_style(ui: &mut egui::Ui, profile: &mut Profile) {
    ui.label("Font style");
    egui::ComboBox::from_id_salt("font-mode")
        .width(230.0)
        .selected_text(mode_name(profile.font_mode))
        .show_ui(ui, |ui| {
            for mode in [FontMode::Automatic, FontMode::TrueType, FontMode::Original] {
                if ui
                    .selectable_label(profile.font_mode == mode, mode_name(mode))
                    .clicked()
                {
                    profile.apply_font_mode(mode);
                }
            }
        })
        .response
        .on_hover_text(
            "Applies to all font groups and resets custom typeface choices. Custom sizes are kept.",
        );
    if profile
        .fonts
        .values()
        .any(|rule| rule.replace_bitmap.is_some() || rule.family.is_some())
    {
        ui.weak("Custom typefaces").on_hover_text(
            "Some groups override this style. Select the style again to apply it to every group.",
        );
    }
}

fn default_scale(ui: &mut egui::Ui, profile: &mut Profile) {
    ui.label("Text size")
        .on_hover_text("Shared by all font groups. Reset to follow the global UI scale.");
    let mut scale = profile.font_scale.unwrap_or(profile.scale);
    if settings::scale(ui, &mut scale, "×") {
        profile.font_scale = Some(scale);
    }
    if profile.font_scale.is_some() {
        if reset_button(ui, "Use UI scale").clicked() {
            profile.font_scale = None;
        }
    } else {
        ui.weak("From UI");
    }
}

fn reset_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let response = ui
        .add(egui::Button::new(icons::RESET).small().frame(false))
        .on_hover_text(label);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), label));
    response
}

fn size_text(size: Option<f64>) -> String {
    size.map_or_else(|| "—".into(), |size| format!("{size}"))
}

fn language_name(language: &str) -> &str {
    match language {
        "l_english" => "English",
        "l_russian" => "Russian",
        "l_french" => "French",
        "l_german" => "German",
        "l_spanish" => "Spanish",
        "l_polish" => "Polish",
        "l_braz_por" => "Portuguese (Brazil)",
        "l_simp_chinese" => "Chinese (Simplified)",
        "l_japanese" => "Japanese",
        "l_korean" => "Korean",
        _ => language.strip_prefix("l_").unwrap_or(language),
    }
}

fn summary(
    ui: &mut egui::Ui,
    group: &Group,
    language: &str,
    scale: Scale,
    rule: &FontRule,
    mode: FontMode,
) {
    ui.columns(2, |columns| {
        for (index, (title, font)) in [
            ("Source", group.original(language)),
            ("Generated", group.resolve(language, rule, scale, mode)),
        ]
        .into_iter()
        .enumerate()
        {
            theme::frame(theme::SURFACE, 14)
                .corner_radius(6)
                .show(&mut columns[index], |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.weak(title);
                    ui.add(
                        egui::Label::new(RichText::new(font.family).strong().color(
                            if index == 1 {
                                theme::MINT
                            } else {
                                ui.visuals().text_color()
                            },
                        ))
                        .wrap(),
                    );
                    let kind = if font.ttf { "TrueType" } else { "Bitmap atlas" };
                    ui.label(format!("{kind} · size {}", size_text(font.size)));
                });
        }
    });
}

fn group_controls(
    ui: &mut egui::Ui,
    catalog: &Catalog,
    group: &Group,
    language: &str,
    scale: Scale,
    rule: &mut FontRule,
    mode: FontMode,
) {
    ui.heading(&group.name);
    if let Some(note) = group.usage_note() {
        ui.label(note);
    }
    area_summary(ui, group, 6);
    ui.small("Changes to this group affect every place that uses it.");
    summary(ui, group, language, scale, rule, mode);
    ui.add_space(8.0);
    ui.collapsing(
        format!("Where used · references: {}", group.usages.len()),
        |ui| {
            usage_hint(ui);
            area_summary(ui, group, usize::MAX);
            usage_list(ui, group, usize::MAX);
        },
    );
    ui.add_space(12.0);
    theme::section(ui, "Typeface");
    family_controls(ui, catalog, group, language, rule);
    rendering_hint(ui, group, language, rule, mode);
    ui.add_space(8.0);
    theme::section(ui, "Sizing");
    settings::field(ui, "Group scale", &mut rule.scale, scale, |ui, value| {
        settings::scale(ui, value, "×");
    });
    let inherited = FontRule {
        replace_bitmap: rule.replace_bitmap,
        ..Default::default()
    };
    let base = group
        .resolve(language, &inherited, scale, mode)
        .base_size
        .unwrap_or(16.0);
    let mut override_size = rule.base_size.map(f64::from);
    settings::field(ui, "Base size", &mut override_size, base, |ui, value| {
        ui.add(
            egui::DragValue::new(value)
                .range(1.0..=512.0)
                .clamp_existing_to_range(false)
                .speed(0.5),
        );
    });
    rule.base_size = override_size.and_then(|raw| FontSize::try_from(raw).ok());
    ui.add_space(8.0);
    ui.collapsing("Details", |ui| {
        let result = group.resolve(language, rule, scale, mode);
        ui.label(format!("Base {} × scale {:.2} = {} (rounded down)", size_text(result.base_size), rule.scale.unwrap_or(scale).get(), size_text(result.size)));
        if let Some(spec) = fonts::replacement(&group.name) {
            ui.label(format!("Default bitmap replacement: {}, base size {}", spec.family, spec.base_size));
        }
        ui.small("Generated font definitions are shown above. Widget scaling can affect their appearance in the game.");
    });
    ui.separator();
    if ui
        .add_enabled(
            *rule != FontRule::default(),
            icons::button(icons::RESET, "Reset group"),
        )
        .clicked()
    {
        *rule = FontRule::default();
    }
}

fn usage_hint(ui: &mut egui::Ui) {
    ui.small("Areas are recognized from GUI files and widget names; typical roles describe the base game. Labels marked inferred rely on built-in group names. References use the game and enabled mods; engine defaults and runtime assignments are not included.");
}

fn usage_tooltip(ui: &mut egui::Ui, group: &Group) {
    ui.strong(&group.name);
    if let Some(note) = group.usage_note() {
        ui.label(note);
    }
    if group.usage_areas().next().is_some() {
        area_summary(ui, group, 8);
    } else {
        usage_list(ui, group, 6);
    }
}

fn matches_area(group: &Group, area: Option<UsageArea>) -> bool {
    area.is_none_or(|area| group.uses_area(area))
}

fn area_summary(ui: &mut egui::Ui, group: &Group, limit: usize) {
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

fn usage_list(ui: &mut egui::Ui, group: &Group, limit: usize) {
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

fn rendering_hint(
    ui: &mut egui::Ui,
    group: &Group,
    language: &str,
    rule: &FontRule,
    mode: FontMode,
) {
    let language_name = language_name(language);
    if let Some(native) = group.native(language) {
        let family = rule.family.as_deref().unwrap_or(&native.family);
        ui.small(format!(
            "{language_name}: TrueType ({family}). This group has a TTF definition in the game/mod."
        ));
    } else if group.replacement_enabled(language, rule, mode) {
        let family = group.replacement_family(rule);
        ui.small(format!(
            "{language_name}: bitmap replaced with TrueType ({family})."
        ));
    } else if group.bitmap {
        let (family, _) = group.bitmap_info(language);
        ui.small(format!("{language_name}: bitmap atlas ({family}). No TTF is assigned to this group for this language. Select a TTF family to replace the bitmap."));
    } else {
        ui.small(format!(
            "{language_name}: no source font definition was found for this group."
        ));
    }
}

fn family_controls(
    ui: &mut egui::Ui,
    catalog: &Catalog,
    group: &Group,
    language: &str,
    rule: &mut FontRule,
) {
    egui::Grid::new("font-typeface-fields")
        .num_columns(3).min_col_width(130.0).spacing([8.0, 9.0])
        .show(ui, |ui| {
            if group.bitmap {
                ui.label("Bitmap fallback");
                egui::ComboBox::from_id_salt("bitmap-replacement").width(260.0)
                    .selected_text(match rule.replace_bitmap {
                        None => "Use overall font style",
                        Some(false) => "Scale original bitmap",
                        Some(true) => "Replace with TrueType",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut rule.replace_bitmap, None, "Use overall font style");
                        ui.selectable_value(&mut rule.replace_bitmap, Some(false), "Scale original bitmap");
                        ui.selectable_value(&mut rule.replace_bitmap, Some(true), "Replace with TrueType");
                    }).response.on_hover_text("Applies when this language has no TTF assigned. Use overall font style inherits the selected preset; Scale original bitmap enlarges the atlas. Existing TTF definitions remain active.");
                if rule.replace_bitmap.is_some() && reset_button(ui, "Reset bitmap fallback").clicked() {
                    rule.replace_bitmap = None;
                }
                ui.end_row();
            }
            ui.label("TTF family");
            ui.add_enabled_ui(group.bitmap || group.native(language).is_some(), |ui| {
                egui::ComboBox::from_id_salt("font-family").width(260.0)
                    .selected_text(rule.family.as_deref().unwrap_or("Default (game/mod or preset)"))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut rule.family, None, "Default (game/mod or preset)");
                        for family in &catalog.families {
                            if ui.selectable_label(rule.family.as_ref() == Some(family), family).clicked() {
                                rule.family = Some(family.clone());
                                if group.bitmap {
                                    rule.replace_bitmap = Some(true);
                                }
                            }
                        }
                    }).response.on_hover_text("Selecting a family also enables TrueType replacement for bitmap fallbacks. Default uses the assigned TTF family or the built-in replacement.");
            });
            if rule.family.is_some() && reset_button(ui, "Reset TTF family").clicked() {
                rule.family = None;
            }
            ui.end_row();
        });
}
#[cfg(test)]
mod tests;
