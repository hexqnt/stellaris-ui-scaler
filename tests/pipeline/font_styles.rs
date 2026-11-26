use super::{fonts_tests::font_fixture, *};
use stellaris_ui_scaler::config::FontMode;

fn localized_bitmap_fixture() -> Temp {
    let fixture = font_fixture();
    let source = fixture.0.join("localization");
    write(
        &source,
        "interface/fonts.gfx",
        r#"bitmapfonts = {
        bitmapfont = { name = cg_16b fontfiles = { "gfx/fonts/cg" } }
        bitmapfont_override = { name = cg_16b fontfiles = { "gfx/fonts/3A_small" } languages = { l_russian } }
        bitmapfont = { name = malgun_goth_24 path = "gfx/fonts/cg" }
        bitmapfont_override = { name = malgun_goth_24 ttf_font = OriginalBody ttf_size = 24 languages = { l_english } }
        bitmapfont_override = { name = malgun_goth_24 fontfiles = { "gfx/fonts/3A_medium" } languages = { l_russian } }
        bitmapfont = { name = large_title_font }
        bitmapfont_override = { name = large_title_font ttf_font = OriginalBody ttf_size = 28 languages = { l_russian } }
    }"#,
    );
    for (name, size, face) in [
        ("3A_small", 16, "Localized body"),
        ("3A_medium", 18, "Localized header"),
    ] {
        write(
            &source,
            &format!("gfx/fonts/{name}.fnt"),
            &format!(
                "info face=\"{face}\" size={size}\ncommon lineHeight={size} base=13 scaleW=8 scaleH=4 pages=1\npage id=0 file=\"{name}.png\"\nchar id=1040 x=1 y=1 width=3 height=2 xoffset=-1 yoffset=0 xadvance=4 page=0\n"
            ),
        );
        fs::write(
            source.join(format!("gfx/fonts/{name}.dds")),
            textures::resource_dds(&image::RgbaImage::from_pixel(
                8,
                4,
                image::Rgba([25, 80, 140, 220]),
            ))
            .unwrap(),
        )
        .unwrap();
    }
    fixture
}

fn language_font<'node, 'source>(
    root: &'node Node<'source>,
    name: &str,
    language: &str,
) -> &'node Node<'source> {
    root.children()
        .iter()
        .rev()
        .find(|node| {
            node.named() == Some(name)
                && node.key == Some("bitmapfont_override")
                && node.field("languages").is_some_and(|languages| {
                    languages
                        .children()
                        .iter()
                        .any(|node| node.atom() == Some(language))
                })
        })
        .unwrap()
}

#[test]
fn localized_bitmap_overrides_scale_automatically_without_changing_the_typeface() {
    use stellaris_ui_scaler::config::FontRule;
    let fixture = localized_bitmap_fixture();
    let sources = vec![fixture.0.join("localization")];
    let control = Control::default();
    let plan =
        compiler::plan_with_mods(&fixture.0.join("game"), &sources, &control, &mut |_| {}).unwrap();
    let output = fixture.0.join("compiled");
    for (factor, body, header, title) in [
        (0.5, 8.0, 9.0, "14"),
        (1.0, 16.0, 18.0, "28"),
        (1.5, 24.0, 27.0, "42"),
        (1.9, 30.0, 34.0, "53"),
        (2.0, 32.0, 36.0, "56"),
        (3.0, 48.0, 54.0, "84"),
    ] {
        let font_scale = Scale::try_from(factor).unwrap();
        let profile = Profile {
            scale: Scale::try_from(2.0).unwrap(),
            font_scale: Some(font_scale),
            source_mods: sources.clone(),
            ..Profile::default()
        };
        build_mod(&plan, &profile, &output).unwrap();
        let gfx = fs::read_to_string(output.join("interface/fonts.gfx")).unwrap();
        let doc = Document::parse(&gfx).unwrap();
        for (name, stem, expected, face) in [
            ("cg_16b", "3A_small", body, "Localized body"),
            ("malgun_goth_24", "3A_medium", header, "Localized header"),
        ] {
            let variant = language_font(&doc.nodes[0], name, "l_russian");
            assert!(variant.field("ttf_font").is_none());
            let path = variant.field("fontfiles").unwrap().children()[0]
                .atom()
                .unwrap();
            assert_eq!(path, format!("gfx/fonts/ui_scaler/{name}/{stem}"));
            let metrics = fs::read_to_string(output.join(format!("{path}.fnt"))).unwrap();
            let font = fonts::bitmap::Font::parse(&metrics).unwrap();
            assert_eq!(font.size, expected);
            assert!(metrics.contains(&format!("face=\"{face}\"")));
            assert!(metrics.contains("id=1040"));
            assert!(metrics.contains(&format!("file=\"{stem}.dds\"")));
            let atlas =
                textures::decode(&fs::read(output.join(format!("{path}.dds"))).unwrap()).unwrap();
            assert_eq!(atlas.dimensions(), font.dimensions);
            let group = plan
                .fonts
                .groups
                .iter()
                .find(|group| group.name == name)
                .unwrap();
            let rule = FontRule::default();
            let resolved = group.resolve("l_russian", &rule, font_scale, FontMode::Automatic);
            assert_eq!(resolved.size, Some(expected));
            assert!(!resolved.ttf);
        }
        let title_font = language_font(&doc.nodes[0], "large_title_font", "l_russian");
        assert_eq!(
            title_font.field("ttf_font").unwrap().atom(),
            Some("OriginalBody")
        );
        assert_eq!(title_font.field("ttf_size").unwrap().atom(), Some(title));
    }
}

#[test]
fn localized_bitmap_overrides_respect_group_scale_and_explicit_ttf_replacement() {
    use stellaris_ui_scaler::config::FontRule;
    let fixture = localized_bitmap_fixture();
    let control = Control::default();
    let source_mods = vec![fixture.0.join("localization")];
    let plan =
        compiler::plan_with_mods(&fixture.0.join("game"), &source_mods, &control, &mut |_| {})
            .unwrap();
    let mut profile = Profile {
        font_scale: Some(Scale::try_from(1.9).unwrap()),
        source_mods,
        ..Profile::default()
    };
    profile.fonts.insert(
        "cg_16b".into(),
        FontRule {
            scale: Some(Scale::try_from(1.25).unwrap()),
            ..Default::default()
        },
    );
    let output = fixture.0.join("compiled");
    build_mod(&plan, &profile, &output).unwrap();
    let metrics =
        fs::read_to_string(output.join("gfx/fonts/ui_scaler/cg_16b/3A_small.fnt")).unwrap();
    assert_eq!(fonts::bitmap::Font::parse(&metrics).unwrap().size, 20.0);
    profile.fonts.insert(
        "cg_16b".into(),
        FontRule {
            replace_bitmap: Some(true),
            ..Default::default()
        },
    );
    build_mod(&plan, &profile, &output).unwrap();
    assert!(
        !output
            .join("gfx/fonts/ui_scaler/cg_16b/3A_small.fnt")
            .exists()
    );
    let gfx = fs::read_to_string(output.join("interface/fonts.gfx")).unwrap();
    let doc = Document::parse(&gfx).unwrap();
    let russian = language_font(&doc.nodes[0], "cg_16b", "l_russian");
    assert_eq!(
        russian.field("ttf_font").unwrap().atom(),
        Some("Easter_normal")
    );
    assert_eq!(russian.field("ttf_size").unwrap().atom(), Some("24"));
}

#[test]
fn overall_font_styles_handle_localized_bitmaps_and_preserve_assigned_mod_ttf() {
    use stellaris_ui_scaler::config::FontRule;
    let fixture = localized_bitmap_fixture();
    let control = Control::default();
    let sources = vec![fixture.0.join("localization")];
    let plan =
        compiler::plan_with_mods(&fixture.0.join("game"), &sources, &control, &mut |_| {}).unwrap();
    let mut profile = Profile {
        font_scale: Some(Scale::try_from(1.9).unwrap()),
        source_mods: sources,
        ..Profile::default()
    };
    let output = fixture.0.join("compiled");
    for mode in [FontMode::Automatic, FontMode::TrueType, FontMode::Original] {
        profile.apply_font_mode(mode);
        build_mod(&plan, &profile, &output).unwrap();
        let gfx = fs::read_to_string(output.join("interface/fonts.gfx")).unwrap();
        let doc = Document::parse(&gfx).unwrap();
        for (name, family, size) in [
            ("cg_16b", "Easter_normal", "24"),
            ("malgun_goth_24", "Easter_header", "34"),
        ] {
            let russian = language_font(&doc.nodes[0], name, "l_russian");
            if mode == FontMode::TrueType {
                assert_eq!(russian.field("ttf_font").unwrap().atom(), Some(family));
                assert_eq!(russian.field("ttf_size").unwrap().atom(), Some(size));
            } else {
                assert!(russian.field("ttf_font").is_none());
                let path = russian.field("fontfiles").unwrap().children()[0]
                    .atom()
                    .unwrap();
                assert!(output.join(format!("{path}.fnt")).is_file());
            }
            let group = plan
                .fonts
                .groups
                .iter()
                .find(|group| group.name == name)
                .unwrap();
            assert_eq!(
                group
                    .resolve(
                        "l_russian",
                        &FontRule::default(),
                        profile.font_scale.unwrap(),
                        mode
                    )
                    .ttf,
                mode == FontMode::TrueType
            );
        }
        let title = language_font(&doc.nodes[0], "large_title_font", "l_russian");
        assert_eq!(
            title.field("ttf_font").unwrap().atom(),
            Some("OriginalBody")
        );
        assert_eq!(title.field("ttf_size").unwrap().atom(), Some("53"));
        let english = language_font(&doc.nodes[0], "malgun_goth_24", "l_english");
        assert_eq!(
            english.field("ttf_font").unwrap().atom(),
            Some("OriginalBody")
        );
        assert_eq!(english.field("ttf_size").unwrap().atom(), Some("45"));
    }
}

#[test]
fn individual_font_rules_can_override_the_overall_style() {
    use stellaris_ui_scaler::config::FontRule;
    let fixture = localized_bitmap_fixture();
    let sources = vec![fixture.0.join("localization")];
    let control = Control::default();
    let plan =
        compiler::plan_with_mods(&fixture.0.join("game"), &sources, &control, &mut |_| {}).unwrap();
    let output = fixture.0.join("compiled");
    let mut profile = Profile {
        font_mode: FontMode::TrueType,
        source_mods: sources,
        ..Profile::default()
    };
    profile.fonts.insert(
        "cg_16b".into(),
        FontRule {
            replace_bitmap: Some(false),
            ..Default::default()
        },
    );
    build_mod(&plan, &profile, &output).unwrap();
    let gfx = fs::read_to_string(output.join("interface/fonts.gfx")).unwrap();
    let doc = Document::parse(&gfx).unwrap();
    assert!(
        language_font(&doc.nodes[0], "cg_16b", "l_russian")
            .field("fontfiles")
            .is_some()
    );
    assert!(
        language_font(&doc.nodes[0], "malgun_goth_24", "l_russian")
            .field("ttf_font")
            .is_some()
    );
    profile.font_mode = FontMode::Original;
    profile.fonts.insert(
        "cg_16b".into(),
        FontRule {
            replace_bitmap: Some(true),
            ..Default::default()
        },
    );
    build_mod(&plan, &profile, &output).unwrap();
    let gfx = fs::read_to_string(output.join("interface/fonts.gfx")).unwrap();
    let doc = Document::parse(&gfx).unwrap();
    assert!(
        language_font(&doc.nodes[0], "cg_16b", "l_russian")
            .field("ttf_font")
            .is_some()
    );
    assert!(
        language_font(&doc.nodes[0], "malgun_goth_24", "l_russian")
            .field("fontfiles")
            .is_some()
    );
}
