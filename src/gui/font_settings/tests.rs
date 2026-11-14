use egui::accesskit::{Action, ActionData};

use crate::gui::tests::{action, node};

use super::*;

#[test]
fn profile_customizations_open_advanced_settings() {
    let mut profile = Profile {
        font_mode: FontMode::TrueType,
        font_scale: Some(Scale::try_from(1.9).unwrap()),
        ..Default::default()
    };
    assert!(!FontSettings::for_profile(&profile).advanced);
    for rule in [
        FontRule::default(),
        FontRule {
            replace_bitmap: Some(false),
            ..Default::default()
        },
        FontRule {
            family: Some("Easter_normal".into()),
            ..Default::default()
        },
        FontRule {
            scale: Some(Scale::try_from(1.5).unwrap()),
            ..Default::default()
        },
        FontRule {
            base_size: Some(FontSize::try_from(20.0).unwrap()),
            ..Default::default()
        },
    ] {
        let customized = rule != FontRule::default();
        profile.fonts.insert("cg_16b".into(), rule);
        assert_eq!(FontSettings::for_profile(&profile).advanced, customized);
    }
}

#[test]
fn loading_profiles_updates_advanced_settings() {
    let temporary = crate::test_support::Temp::new();
    let path = temporary.0.join("profile.toml");
    let mut profile = Profile::default();
    profile.fonts.insert(
        "cg_16b".into(),
        FontRule {
            scale: Some(Scale::try_from(1.5).unwrap()),
            ..Default::default()
        },
    );
    profile.save(&path).unwrap();
    let mut studio = crate::gui::Studio::new(None);
    studio.profile_path = path.display().to_string();
    studio.font_settings.advanced = false;
    studio.load_profile();
    assert!(studio.font_settings.advanced);

    let ctx = egui::Context::default();
    studio.font_settings.advanced = false;
    render(
        &ctx,
        &mut studio.font_settings,
        &mut studio.profile,
        Vec::new(),
    )
    .drop_without_applying_deltas();
    assert!(!studio.font_settings.advanced);
    studio.load_profile();
    assert!(studio.font_settings.advanced);

    Profile::default().save(&path).unwrap();
    studio.load_profile();
    assert!(!studio.font_settings.advanced);
}

fn editor() -> (egui::Context, FontSettings, Profile) {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    icons::install(&ctx).unwrap();
    (
        ctx,
        FontSettings {
            advanced: true,
            ..Default::default()
        },
        Profile {
            font_scale: Some(Scale::try_from(1.9).unwrap()),
            ..Default::default()
        },
    )
}

fn catalog() -> Catalog {
    Catalog {
        languages: vec!["l_english".into(), "l_russian".into()],
        families: vec!["Easter_normal".into()],
        groups: vec![Group {
            name: "cg_16b".into(),
            bitmap: true,
            variants: [("l_english", 16.0), ("l_russian", 13.0)]
                .into_iter()
                .map(|(language, size)| fonts::Variant {
                    family: "Easter_normal".into(),
                    size,
                    languages: vec![language.into()],
                })
                .collect(),
            ..Default::default()
        }],
    }
}

fn render(
    ctx: &egui::Context,
    settings: &mut FontSettings,
    profile: &mut Profile,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    render_catalog(ctx, settings, profile, &catalog(), events)
}

fn render_catalog(
    ctx: &egui::Context,
    settings: &mut FontSettings,
    profile: &mut Profile,
    catalog: &Catalog,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1100.0, 700.0),
            )),
            events,
            ..Default::default()
        },
        |ui| settings.show(ui, catalog, profile),
    )
}

#[test]
fn area_filter_selects_the_matching_font_and_handles_missing_references() {
    let (ctx, mut settings, mut profile) = editor();
    let mut catalog = catalog();
    settings.area = Some(UsageArea::MapResources);
    let mut output = render_catalog(&ctx, &mut settings, &mut profile, &catalog, Vec::new());
    output.textures_delta.clear();
    node(&output, |node| {
        node.value()
            .is_some_and(|text| text.starts_with("No fonts found for this area"))
    });
    output.drop_without_applying_deltas();
    catalog.groups.push(Group {
        name: "hoi_16mbs".into(),
        usages: vec![fonts::Usage {
            area: Some(UsageArea::MapResources),
            file: "interface/mapicons.gui".into(),
            line: 79,
            widget: Some("num".into()),
            kind: "instantTextBoxType".into(),
            property: "font".into(),
        }],
        ..Default::default()
    });
    let mut output = render_catalog(&ctx, &mut settings, &mut profile, &catalog, Vec::new());
    assert_eq!(settings.selected, 1);
    output.textures_delta.clear();
    node(&output, |node| {
        node.label()
            .is_some_and(|label| label.starts_with("hoi_16mbs "))
    });
    assert!(
        output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .all(|(_, node)| !node
                .label()
                .is_some_and(|label| label.starts_with("cg_16b ")))
    );
    output.drop_without_applying_deltas();
    assert!(profile.fonts.is_empty());
}

#[test]
fn expanding_font_usage_shows_the_file_and_widget() {
    let (ctx, mut settings, mut profile) = editor();
    let mut catalog = catalog();
    catalog.groups[0].usages.push(fonts::Usage {
        area: Some(UsageArea::PlanetView),
        file: "interface/planet_view.gui".into(),
        line: 42,
        widget: Some("planet_title".into()),
        kind: "instantTextBoxType".into(),
        property: "font".into(),
    });
    let output = render_catalog(&ctx, &mut settings, &mut profile, &catalog, Vec::new());
    let usage = node(&output, |node| {
        node.label() == Some("Where used · references: 1")
    });
    output.drop_without_applying_deltas();
    render_catalog(
        &ctx,
        &mut settings,
        &mut profile,
        &catalog,
        vec![action(Action::Click, usage, None)],
    )
    .drop_without_applying_deltas();
    let output = render_catalog(&ctx, &mut settings, &mut profile, &catalog, Vec::new());
    node(&output, |node| {
        node.value() == Some("interface/planet_view.gui")
    });
    node(&output, |node| {
        node.value() == Some("Line 42 · planet_title · instantTextBoxType · font")
    });
    output.drop_without_applying_deltas();
    assert!(profile.fonts.is_empty());
}

#[test]
fn editing_and_resetting_base_size_preserves_language_specific_inheritance() {
    let (ctx, mut settings, mut profile) = editor();
    let output = render(&ctx, &mut settings, &mut profile, Vec::new());
    let base = node(&output, |node| {
        node.role() == egui::Role::SpinButton && node.numeric_value() == Some(13.0)
    });
    output.drop_without_applying_deltas();
    assert!(profile.fonts.is_empty());
    render(
        &ctx,
        &mut settings,
        &mut profile,
        vec![action(
            Action::SetValue,
            base,
            Some(ActionData::NumericValue(20.0)),
        )],
    )
    .drop_without_applying_deltas();
    assert_eq!(
        profile.fonts["cg_16b"],
        FontRule {
            base_size: Some(FontSize::try_from(20.0).unwrap()),
            ..Default::default()
        }
    );
    settings.language = "l_english".into();
    let output = render(&ctx, &mut settings, &mut profile, Vec::new());
    let reset = node(&output, |node| node.label() == Some("Reset Base size"));
    output.drop_without_applying_deltas();
    assert_eq!(profile.fonts["cg_16b"].base_size.map(f64::from), Some(20.0));
    render(
        &ctx,
        &mut settings,
        &mut profile,
        vec![action(Action::Click, reset, None)],
    )
    .drop_without_applying_deltas();
    assert!(profile.fonts.is_empty());
    let catalog = catalog();
    let rule = FontRule::default();
    let result = catalog.groups[0].resolve(
        "l_english",
        &rule,
        profile.font_scale.unwrap(),
        FontMode::Automatic,
    );
    assert_eq!(result.base_size, Some(16.0));
    assert_eq!(result.size, Some(30.0));
}

#[test]
fn resetting_default_scale_preserves_group_overrides() {
    let (ctx, mut settings, mut profile) = editor();
    let group_scale = Scale::try_from(2.2).unwrap();
    profile.fonts.insert(
        "cg_16b".into(),
        FontRule {
            scale: Some(group_scale),
            ..Default::default()
        },
    );
    let output = render(&ctx, &mut settings, &mut profile, Vec::new());
    let reset = node(&output, |node| node.label() == Some("Use UI scale"));
    output.drop_without_applying_deltas();
    render(
        &ctx,
        &mut settings,
        &mut profile,
        vec![action(Action::Click, reset, None)],
    )
    .drop_without_applying_deltas();
    assert!(profile.font_scale.is_none());
    assert_eq!(profile.fonts["cg_16b"].scale, Some(group_scale));
}

#[test]
fn choosing_a_ttf_family_switches_the_bitmap_fallback_to_truetype() {
    let (ctx, mut settings, mut profile) = editor();
    let mut catalog = catalog();
    let group = &mut catalog.groups[0];
    group.variants.retain(|variant| {
        !variant
            .languages
            .iter()
            .any(|language| language == "l_russian")
    });
    group.bitmap_variants.push(fonts::BitmapVariant {
        path: "gfx/fonts/3A_small".into(),
        face: Some("3A_small".into()),
        size: Some(16.0),
        languages: vec!["l_russian".into()],
    });
    profile.fonts.insert(
        "cg_16b".into(),
        FontRule {
            replace_bitmap: Some(false),
            ..Default::default()
        },
    );
    let output = render_catalog(&ctx, &mut settings, &mut profile, &catalog, Vec::new());
    let family = node(&output, |node| {
        node.role() == egui::Role::ComboBox && node.value() == Some("Default (game/mod or preset)")
    });
    output.drop_without_applying_deltas();
    render_catalog(
        &ctx,
        &mut settings,
        &mut profile,
        &catalog,
        vec![action(Action::Click, family, None)],
    )
    .drop_without_applying_deltas();
    let output = render_catalog(&ctx, &mut settings, &mut profile, &catalog, Vec::new());
    let option = node(&output, |node| node.label() == Some("Easter_normal"));
    output.drop_without_applying_deltas();
    render_catalog(
        &ctx,
        &mut settings,
        &mut profile,
        &catalog,
        vec![action(Action::Click, option, None)],
    )
    .drop_without_applying_deltas();
    let rule = &profile.fonts["cg_16b"];
    assert_eq!(rule.family.as_deref(), Some("Easter_normal"));
    assert_eq!(rule.replace_bitmap, Some(true));
    let resolved = catalog.groups[0].resolve(
        "l_russian",
        rule,
        profile.font_scale.unwrap(),
        FontMode::Automatic,
    );
    assert!(resolved.ttf);
    assert_eq!(resolved.size, Some(24.0));
    let output = render_catalog(&ctx, &mut settings, &mut profile, &catalog, Vec::new());
    node(&output, |node| node.value() == Some("TrueType · size 24"));
    output.drop_without_applying_deltas();
}

#[test]
fn assigned_mod_ttf_is_used_automatically_and_explained_in_the_ui() {
    let (ctx, mut settings, mut profile) = editor();
    let mut catalog = catalog();
    catalog.families.push("OrbitronCYR".into());
    let group = &mut catalog.groups[0];
    group.name = "large_title_font".into();
    group.variants = vec![fonts::Variant {
        family: "OrbitronCYR".into(),
        size: 28.0,
        languages: vec!["l_russian".into()],
    }];
    let output = render_catalog(&ctx, &mut settings, &mut profile, &catalog, Vec::new());
    node(&output, |node| node.value() == Some("TrueType · size 53"));
    node(&output, |node| {
        node.value()
            .is_some_and(|value| value.contains("TrueType (OrbitronCYR)"))
    });
    output.drop_without_applying_deltas();
    assert!(profile.fonts.is_empty());
}

#[test]
fn simple_font_style_choice_applies_to_all_typefaces_without_opening_advanced_settings() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    icons::install(&ctx).unwrap();
    let mut settings = FontSettings::default();
    let mut profile = Profile::default();
    profile.fonts.insert(
        "cg_16b".into(),
        FontRule {
            replace_bitmap: Some(false),
            family: Some("Easter_normal".into()),
            ..Default::default()
        },
    );
    let output = render(&ctx, &mut settings, &mut profile, Vec::new());
    let nodes = &output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes;
    assert!(
        nodes
            .iter()
            .all(|(_, node)| node.value() != Some("TTF family") && node.label() != Some("cg_16b"))
    );
    let mode = node(&output, |node| {
        node.role() == egui::Role::ComboBox && node.value() == Some("Automatic")
    });
    output.drop_without_applying_deltas();
    render(
        &ctx,
        &mut settings,
        &mut profile,
        vec![action(Action::Click, mode, None)],
    )
    .drop_without_applying_deltas();
    let output = render(&ctx, &mut settings, &mut profile, Vec::new());
    let sharp = node(&output, |node| {
        node.label() == Some("Sharp text (TrueType)")
    });
    output.drop_without_applying_deltas();
    render(
        &ctx,
        &mut settings,
        &mut profile,
        vec![action(Action::Click, sharp, None)],
    )
    .drop_without_applying_deltas();
    assert_eq!(profile.font_mode, FontMode::TrueType);
    assert!(profile.fonts.is_empty());
    assert!(!settings.advanced);
}
