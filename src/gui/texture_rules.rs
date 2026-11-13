use eframe::egui;

use crate::config::{Algorithm, Category, Profile, Scale, TextureRule};

use super::{
    icons,
    settings::{self, field},
};

#[derive(Clone, PartialEq, Eq)]
pub(super) enum Selection {
    Category(Category),
    Directory(String),
    Asset(String),
}

impl Selection {
    pub(super) fn key(&self) -> &str {
        match self {
            Self::Category(category) => category.name(),
            Self::Directory(path) | Self::Asset(path) => path,
        }
    }

    const fn rules<'a>(
        &self,
        profile: &'a mut Profile,
    ) -> &'a mut std::collections::BTreeMap<String, TextureRule> {
        match self {
            Self::Category(_) => &mut profile.categories,
            Self::Directory(_) => &mut profile.directories,
            Self::Asset(_) => &mut profile.assets,
        }
    }

    pub fn set_algorithm(&self, profile: &mut Profile, algorithm: Algorithm) {
        self.rules(profile)
            .entry(self.key().to_owned())
            .or_default()
            .algorithm = Some(algorithm);
    }

    pub fn show(
        &self,
        ui: &mut egui::Ui,
        profile: &mut Profile,
        category: Category,
        frame_size: Option<(u32, u32)>,
        features: Option<&crate::config::ImageFeatures>,
    ) -> bool {
        // Сохраняем запись в карте, чтобы вычислять наследование без перевыделения памяти каждый кадр.
        let mut rule = self
            .rules(profile)
            .get_mut(self.key())
            .map_or_default(std::mem::take);
        let previous = rule;
        let path = match self {
            Self::Category(_) => "",
            Self::Directory(path) | Self::Asset(path) => path.as_str(),
        };
        let policy = profile.texture_policy(path, category, frame_size);
        controls(
            ui,
            &mut rule,
            policy.processing(features).options,
            policy.scale,
        );
        if ui
            .add_enabled(
                rule != TextureRule::default(),
                icons::button(icons::RESET, "Reset all overrides"),
            )
            .clicked()
        {
            rule = TextureRule::default();
        }
        let changed = previous != rule;
        if rule == TextureRule::default() {
            self.rules(profile).remove(self.key());
        } else if let Some(saved) = self.rules(profile).get_mut(self.key()) {
            *saved = rule;
        } else {
            self.rules(profile).insert(self.key().to_owned(), rule);
        }
        changed
    }
}

fn controls(
    ui: &mut egui::Ui,
    rule: &mut TextureRule,
    inherited: crate::config::TextureOptions,
    scale: Scale,
) {
    ui.push_id("texture-rule", |ui| {
        field(ui, "Texture scale", &mut rule.scale, scale, |ui, value| {
            settings::scale(ui, value, "");
        });
        field(
            ui,
            "Algorithm",
            &mut rule.algorithm,
            inherited.algorithm,
            |ui, value| {
                settings::algorithm(ui, "algorithm", 110.0, value);
            },
        );
        field(
            ui,
            "Color space",
            &mut rule.color_space,
            inherited.color_space,
            |ui, value| {
                settings::color_space(ui, "space", 110.0, value);
            },
        );
        for (label, value, inherited) in [
            (
                "Premultiplied alpha",
                &mut rule.premultiplied_alpha,
                inherited.premultiplied_alpha,
            ),
            (
                "Separate frames",
                &mut rule.independent_frames,
                inherited.independent_frames,
            ),
        ] {
            field(ui, label, value, inherited, toggle);
        }
        for (label, value, inherited, range, speed) in [
            (
                "Sharpening",
                &mut rule.sharpening,
                inherited.sharpening,
                0.0..=2.0,
                0.01,
            ),
            (
                "Radius",
                &mut rule.sharpening_radius,
                inherited.sharpening_radius,
                0.1..=5.0,
                0.1,
            ),
        ] {
            field(ui, label, value, inherited, |ui, value| {
                ui.add(egui::DragValue::new(value).range(range).speed(speed));
            });
        }
        field(
            ui,
            "Threshold",
            &mut rule.sharpening_threshold,
            inherited.sharpening_threshold,
            |ui, value| {
                ui.add(egui::DragValue::new(value).range(0..=255));
            },
        );
    });
}

fn toggle(ui: &mut egui::Ui, value: &mut bool) {
    let label = if *value { "On" } else { "Off" };
    ui.toggle_value(value, label);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editing_an_inherited_value_creates_only_that_override_and_reset_restores_inheritance() {
        use crate::gui::tests::{action, node};
        use egui::accesskit::{Action, ActionData};

        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        icons::install(&ctx).unwrap();
        let mut profile = Profile::default();
        let path = "gfx/icons/test.dds";
        let selection = Selection::Asset(path.into());
        profile.directories.insert(
            "gfx/icons".into(),
            TextureRule {
                scale: Some(Scale::try_from(1.5).unwrap()),
                ..Default::default()
            },
        );
        let render = |profile: &mut Profile, events| {
            ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    ui.set_max_width(292.0);
                    selection.show(ui, profile, Category::Icon, Some((32, 32)), None);
                },
            )
        };
        let mut scale_node = None;
        for _ in 0..3 {
            let output = render(&mut profile, Vec::new());
            for (id, node) in &output
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes
            {
                if node.role() == egui::Role::SpinButton && node.numeric_value() == Some(1.5) {
                    scale_node = Some(*id);
                }
            }
            output.drop_without_applying_deltas();
        }
        assert!(profile.assets.is_empty());
        render(
            &mut profile,
            vec![action(
                Action::SetValue,
                scale_node.unwrap(),
                Some(ActionData::NumericValue(2.5)),
            )],
        )
        .drop_without_applying_deltas();
        assert_eq!(
            profile.assets[path],
            TextureRule {
                scale: Some(Scale::try_from(2.5).unwrap()),
                ..Default::default()
            }
        );
        profile.directories.get_mut("gfx/icons").unwrap().scale =
            Some(Scale::try_from(1.75).unwrap());
        assert_eq!(profile.texture_scale(path, Category::Icon).get(), 2.5);
        let output = render(&mut profile, Vec::new());
        let reset = node(&output, |node| node.label() == Some("Reset Texture scale"));
        output.drop_without_applying_deltas();
        render(&mut profile, vec![action(Action::Click, reset, None)])
            .drop_without_applying_deltas();
        assert!(profile.assets.is_empty());
        assert_eq!(profile.texture_scale(path, Category::Icon).get(), 1.75);
    }
}
