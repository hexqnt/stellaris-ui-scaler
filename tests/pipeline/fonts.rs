use stellaris_ui_scaler::config::FontMode;

use super::*;

#[test]
fn font_usage_tracks_explicit_assignments_in_effective_mod_files() {
    let fixture = font_fixture();
    let game = fixture.0.join("game");
    let source_mod = fixture.0.join("source-mod");
    let gui = "\u{feff}guiTypes = {\r\n containerWindowType = { name = окно\r\n  instantTextBoxType = { name = title FONT = \"cg_16b\" }\r\n  font = cg_16b\r\n  buttonType = { name = confirm buttonFont = cg_16b }\r\n  # font = hoi_16mbs\r\n  text = \"font = hoi_16mbs\"\r\n  iconType = { name = cg_16b spriteType = cg_16b }\r\n  instantTextBoxType = { font = unknown_font }\r\n }\r\n}";
    write(&game, "interface/usage.gui", gui);
    write(
        &game,
        "interface/other.gui",
        "textBoxType = { font = cg_16b }",
    );
    write(
        &source_mod,
        "interface/usage.gui",
        "buttonType = { name = mod_button buttonFont = jura }",
    );
    let control = Control::default();
    let plan = scan_game(&game).unwrap();
    let group = plan
        .fonts
        .groups
        .iter()
        .find(|g| g.name == "cg_16b")
        .unwrap();
    let references: Vec<_> = group
        .usages
        .iter()
        .map(|usage| {
            (
                usage.file.as_ref(),
                usage.line,
                usage.widget.as_deref(),
                usage.kind.as_str(),
                usage.property.as_str(),
            )
        })
        .collect();
    assert_eq!(
        references,
        [
            ("interface/other.gui", 1, None, "textBoxType", "font"),
            (
                "interface/usage.gui",
                3,
                Some("title"),
                "instantTextBoxType",
                "FONT"
            ),
            (
                "interface/usage.gui",
                4,
                Some("окно"),
                "containerWindowType",
                "font"
            ),
            (
                "interface/usage.gui",
                5,
                Some("confirm"),
                "buttonType",
                "buttonFont"
            ),
        ]
    );
    assert!(
        plan.fonts
            .groups
            .iter()
            .find(|g| g.name == "hoi_16mbs")
            .unwrap()
            .usages
            .is_empty()
    );
    let plan = compiler::plan_with_mods(&game, &[source_mod], &control, &mut |_| {}).unwrap();
    let group = plan
        .fonts
        .groups
        .iter()
        .find(|g| g.name == "cg_16b")
        .unwrap();
    assert_eq!(group.usages.len(), 1);
    assert_eq!(group.usages[0].file.as_ref(), "interface/other.gui");
    let group = plan.fonts.groups.iter().find(|g| g.name == "jura").unwrap();
    assert_eq!(group.usages.len(), 1);
    assert_eq!(group.usages[0].widget.as_deref(), Some("mod_button"));
}

#[test]
fn combined_font_pass_preserves_sequential_definitions_and_bitmap_outputs() {
    use stellaris_ui_scaler::config::FontRule;
    let fixture = font_fixture();
    let plan = scan_game(&fixture.0.join("game")).unwrap();
    let profile = Profile {
        font_scale: Some(Scale::try_from(1.5).unwrap()),
        fonts: BTreeMap::from([(
            "cg_16b".into(),
            FontRule {
                replace_bitmap: Some(false),
                ..Default::default()
            },
        )]),
        ..Default::default()
    };
    let source = &plan.sources["interface/fonts.gfx"];
    let mut generated = BTreeMap::new();
    let expected = fonts::bitmap::compile(source, &plan.game, &profile, &mut |path, bytes| {
        generated.insert(path.to_owned(), bytes.to_vec());
        Ok(())
    })
    .unwrap();
    let expected = fonts::transform_configured(
        &expected,
        profile.font_scale.unwrap(),
        &profile.fonts,
        &plan.fonts,
        profile.font_mode,
    )
    .unwrap();
    let output = fixture.0.join("mod");
    build_mod(&plan, &profile, &output).unwrap();
    assert_eq!(
        fs::read_to_string(output.join("interface/fonts.gfx")).unwrap(),
        expected
    );
    assert_eq!(generated.len(), 2);
    for (path, bytes) in generated {
        assert_eq!(fs::read(output.join(path)).unwrap(), bytes);
    }
}

#[test]
fn font_sizes_round_down_in_definitions_and_bitmap_overrides() {
    let source = r#"font = { ttf_size = "25" } font = { ttf_size = 26 } font = { ttf_size = 18.5 } font = { ttf_size = @font_size } bitmapfonts = { bitmapfont = { name = "hoi_16mbs" } bitmapfont = { name = "jura" } }"#;
    for (factor, quoted, unquoted, fractional, hoi, jura) in [
        (1.0, 25, 26, 18, 16, 36),
        (1.1, 27, 28, 20, 17, 39),
        (1.5, 37, 39, 27, 24, 54),
        (2.0, 50, 52, 37, 32, 72),
    ] {
        let text = fonts::transform(source, Scale::try_from(factor).unwrap()).unwrap();
        assert!(text.contains(&format!("font = {{ ttf_size = \"{quoted}\" }}")));
        assert!(text.contains(&format!("font = {{ ttf_size = {unquoted} }}")));
        assert!(text.contains(&format!("font = {{ ttf_size = {fractional} }}")));
        assert!(text.contains("ttf_size = @font_size"));
        assert!(text.contains(&format!(
            "name = \"hoi_16mbs\" ttf_font = \"Easter_normal\" ttf_size = \"{hoi}\""
        )));
        assert!(text.contains(&format!(
            "name = \"jura\" ttf_font = \"Jura_ttf\" ttf_size = \"{jura}\""
        )));
    }
}

#[test]
fn font_rules_roundtrip_and_control_native_and_replacement_variants() {
    use stellaris_ui_scaler::config::{FontRule, FontSize};
    let fixture = font_fixture();
    let root = fixture.0.join("game");
    let plan = scan_game(&root).unwrap();
    let mut profile = Profile {
        scale: Scale::try_from(2.0).unwrap(),
        ..Profile::default()
    };
    profile.fonts.insert(
        "cg_16b".into(),
        FontRule {
            replace_bitmap: Some(false),
            family: Some("OriginalBody".into()),
            scale: Some(Scale::try_from(1.75).unwrap()),
            base_size: Some(FontSize::try_from(15.0).unwrap()),
        },
    );
    profile.fonts.insert(
        "jura".into(),
        FontRule {
            replace_bitmap: Some(false),
            ..FontRule::default()
        },
    );
    profile.fonts.insert(
        "hoi_16mbs".into(),
        FontRule {
            replace_bitmap: Some(true),
            family: Some("OriginalBody".into()),
            scale: Some(Scale::try_from(1.2).unwrap()),
            base_size: Some(FontSize::try_from(17.0).unwrap()),
        },
    );
    let path = fixture.0.join("fonts.toml");
    profile.save(&path).unwrap();
    let loaded = Profile::load(&path).unwrap();
    assert_eq!(loaded.fonts, profile.fonts);
    let output = fixture.0.join("mod");
    build_mod(&plan, &loaded, &output).unwrap();
    let text = fs::read_to_string(output.join("interface/fonts.gfx")).unwrap();
    let doc = Document::parse(&text).unwrap();
    let root = &doc.nodes[0];
    let cg = root
        .children()
        .iter()
        .find(|node| node.key == Some("bitmapfont_override") && node.named() == Some("cg_16b"))
        .unwrap();
    assert_eq!(cg.field("ttf_font").unwrap().atom(), Some("OriginalBody"));
    assert_eq!(cg.field("ttf_size").unwrap().atom(), Some("26"));
    assert_eq!(root.children().iter().filter(|node| node.key == Some("bitmapfont_override") && node.named() == Some("cg_16b")).count(), 1);
    assert!(
        !root
            .children()
            .iter()
            .any(|node| node.key == Some("bitmapfont_override") && node.named() == Some("jura"))
    );
    let hoi = root
        .children()
        .iter()
        .find(|node| node.key == Some("bitmapfont_override") && node.named() == Some("hoi_16mbs"))
        .unwrap();
    assert_eq!(hoi.field("ttf_font").unwrap().atom(), Some("OriginalBody"));
    assert_eq!(hoi.field("ttf_size").unwrap().atom(), Some("20"));
    for (name, expected_size, ttf) in [
        ("cg_16b", 26.0, true),
        ("hoi_16mbs", 20.0, true),
        ("jura", 96.0, false),
    ] {
        let group = plan
            .fonts
            .groups
            .iter()
            .find(|group| group.name == name)
            .unwrap();
        let resolved = group.resolve(
            "l_russian",
            &profile.fonts[name],
            profile.scale,
            FontMode::Automatic,
        );
        assert_eq!(resolved.size, Some(expected_size));
        assert_eq!(resolved.ttf, ttf);
    }
    let cg = plan
        .fonts
        .groups
        .iter()
        .find(|group| group.name == "cg_16b")
        .unwrap();
    assert_eq!(cg.bitmap_size, Some(16.0));
    assert_eq!(cg.native("l_russian").unwrap().size, 13.0);
    let english = cg.resolve(
        "l_english",
        &profile.fonts["cg_16b"],
        profile.scale,
        FontMode::Automatic,
    );
    assert!(!english.ttf);
    assert_eq!(english.size, Some(26.0));
}

#[test]
fn invalid_font_settings_fail_before_output_is_created() {
    use stellaris_ui_scaler::config::{FontRule, FontSize};
    assert!(FontSize::try_from(f64::NAN).is_err());
    assert!(FontSize::try_from(0.0).is_err());
    assert!(FontSize::try_from(513.0).is_err());
    let fixture = fixture();
    let root = fixture.0.join("game");
    write(
        &root,
        "interface/fonts.gfx",
        r#"bitmapfonts = { bitmapfont = { name = "jura" } }"#,
    );
    let plan = scan_game(&root).unwrap();
    let mut profile = Profile::default();
    profile.fonts.insert(
        "jura".into(),
        FontRule {
            family: Some("MissingFamily".into()),
            ..FontRule::default()
        },
    );
    let output = fixture.0.join("invalid");
    let error = build_mod(&plan, &profile, &output).unwrap_err();
    assert!(error.to_string().contains("Unknown TTF family"));
    assert!(!output.exists());
    profile.fonts.clear();
    profile
        .fonts
        .insert("MissingGroup".into(), FontRule::default());
    assert!(plan.fonts.check_rules(&profile.fonts).is_err());
}

pub fn font_fixture() -> Temp {
    let fixture = fixture();
    let root = fixture.0.join("game");
    write(
        &root,
        "fonts/fonts.asset",
        r#"font = { name = "Easter_normal" } font = { name = "OriginalBody" } font = { name = "Jura_ttf" }"#,
    );
    write(
        &root,
        "interface/fonts.gfx",
        r#"bitmapfonts = {
        bitmapfont = { name = "cg_16b" path = "gfx/fonts/cg" }
        bitmapfont_override = { name = "cg_16b" ttf_font = "Easter_normal" ttf_size = "13" languages = { "l_russian" "l_polish" } }
        bitmapfont = { name = "jura" path = "gfx/fonts/jura" }
        bitmapfont = { name = "hoi_16mbs" path = "gfx/fonts/hoi" }
    }"#,
    );
    for (name, size) in [("cg", 16), ("jura", 48), ("hoi", 16)] {
        write(
            &root,
            &format!("gfx/fonts/{name}.fnt"),
            &format!(
                "info face=\"Original {name}\" size={size}\ncommon lineHeight={size} base=13 scaleW=8 scaleH=4 pages=1\nchar id=65 x=1 y=1 width=3 height=2 xoffset=-1 yoffset=0 xadvance=4 page=0\nkerning first=65 second=66 amount=-1\n"
            ),
        );
    }
    for name in ["cg", "jura", "hoi"] {
        let atlas = image::RgbaImage::from_pixel(8, 4, image::Rgba([30, 70, 120, 160]));
        fs::write(
            root.join(format!("gfx/fonts/{name}.dds")),
            textures::resource_dds(&atlas).unwrap(),
        )
        .unwrap();
    }
    fixture
}

#[test]
fn font_catalog_ignores_invalid_native_sizes_consistently_with_transformation() {
    let fixture = font_fixture();
    let root = fixture.0.join("game");
    let game = Installation::open(&root).unwrap();
    let original = game.text("interface/fonts.gfx").unwrap();
    for size in ["NaN", "inf", "-2", "0", "@size"] {
        let source = format!(
            "{original}\nbitmapfonts = {{ bitmapfont_override = {{ name = cg_16b ttf_font = Easter_normal ttf_size = {size} languages = {{ l_russian }} }} }}"
        );
        let sources = BTreeMap::from([("interface/fonts.gfx".into(), source.clone())]);
        let catalog = fonts::Catalog::load(&game, &sources).unwrap();
        let group = catalog
            .groups
            .iter()
            .find(|group| group.name == "cg_16b")
            .unwrap();
        assert_eq!(group.native("l_russian").unwrap().size, 13.0, "{size}");
        let text = fonts::transform_configured(
            &source,
            Scale::try_from(2.0).unwrap(),
            &BTreeMap::new(),
            &catalog,
            FontMode::Automatic,
        )
        .unwrap();
        assert!(text.contains(&format!("ttf_size = {size}")));
        assert!(text.contains("ttf_size = \"26\""));
    }
}

#[test]
fn bitmap_compilation_propagates_writer_errors_without_later_writes() {
    use stellaris_ui_scaler::config::FontRule;
    let fixture = font_fixture();
    let root = fixture.0.join("game");
    let game = Installation::open(&root).unwrap();
    let source = game.text("interface/fonts.gfx").unwrap();
    let mut profile = Profile::default();
    for name in ["cg_16b", "jura", "hoi_16mbs"] {
        profile.fonts.insert(
            name.into(),
            FontRule {
                replace_bitmap: Some(false),
                ..FontRule::default()
            },
        );
    }
    let mut writes = Vec::new();
    let error = fonts::bitmap::compile(&source, &game, &profile, &mut |path, _| {
        writes.push(path.to_owned());
        anyhow::bail!("Writer refused bitmap output")
    })
    .unwrap_err();
    assert_eq!(error.to_string(), "Writer refused bitmap output");
    assert_eq!(writes, ["gfx/fonts/ui_scaler/cg_16b/cg.fnt"]);
}

#[test]
fn font_catalog_preserves_language_specific_bitmap_sizes() {
    use stellaris_ui_scaler::config::FontRule;
    let fixture = font_fixture();
    let root = fixture.0.join("game");
    let mut source = fs::read_to_string(root.join("interface/fonts.gfx")).unwrap();
    source.push_str(r#"bitmapfonts = {
        bitmapfont = { name = "load_screen" path = "gfx/fonts/hoi" }
        bitmapfont_override = { name = "load_screen" fontfiles = { "gfx/fonts/jura" } languages = { "l_russian" } }
    }"#);
    write(&root, "interface/fonts.gfx", &source);
    let plan = scan_game(&root).unwrap();
    let group = plan
        .fonts
        .groups
        .iter()
        .find(|group| group.name == "load_screen")
        .unwrap();
    assert_eq!(group.bitmap_info("l_russian").1, Some(48.0));
    assert_eq!(group.bitmap_info("l_english").1, Some(16.0));
    let scale = Scale::try_from(1.5).unwrap();
    assert_eq!(
        group
            .resolve(
                "l_russian",
                &FontRule::default(),
                scale,
                FontMode::Automatic
            )
            .size,
        Some(72.0)
    );
    let rule = FontRule {
        replace_bitmap: Some(true),
        family: Some("OriginalBody".into()),
        ..FontRule::default()
    };
    assert_eq!(
        group
            .resolve("l_russian", &rule, scale, FontMode::Automatic)
            .size,
        Some(72.0)
    );
    assert_eq!(
        group
            .resolve("l_english", &rule, scale, FontMode::Automatic)
            .size,
        Some(24.0)
    );
    let mut profile = Profile {
        scale,
        ..Profile::default()
    };
    profile.fonts.insert("load_screen".into(), rule);
    let output = fixture.0.join("mod");
    build_mod(&plan, &profile, &output).unwrap();
    let text = fs::read_to_string(output.join("interface/fonts.gfx")).unwrap();
    let doc = Document::parse(&text).unwrap();
    let mut sizes = BTreeMap::new();
    doc.walk(|node| {
        if node.named() == Some("load_screen") && node.field("ttf_font").is_some() {
            let size = node.field("ttf_size").unwrap().atom().unwrap();
            for language in node.field("languages").unwrap().children() {
                sizes.insert(language.atom().unwrap(), size);
            }
        }
    });
    assert_eq!(sizes["l_russian"], "72");
    assert_eq!(sizes["l_english"], "24");
}

#[test]
fn bitmap_scaling_preserves_metrics_typeface_and_shared_sources() {
    use stellaris_ui_scaler::config::FontRule;
    let fixture = font_fixture();
    let root = fixture.0.join("game");
    write(
        &root,
        "interface/fonts.gfx",
        r#"bitmapfonts = {
        bitmapfont = { name = "hoi_16mbs" path = "gfx/fonts/hoi" }
        bitmapfont = { name = "jura" fontfiles = { "gfx/fonts/hoi" } }
    }"#,
    );
    let original = fs::read(root.join("gfx/fonts/hoi.fnt")).unwrap();
    let plan = scan_game(&root).unwrap();
    let mut profile = Profile {
        scale: Scale::try_from(2.0).unwrap(),
        ..Profile::default()
    };
    profile.fonts.insert(
        "hoi_16mbs".into(),
        FontRule {
            replace_bitmap: Some(false),
            ..FontRule::default()
        },
    );
    profile.fonts.insert(
        "jura".into(),
        FontRule {
            replace_bitmap: Some(false),
            scale: Some(Scale::try_from(1.5).unwrap()),
            ..FontRule::default()
        },
    );
    let output = fixture.0.join("bitmap-mod");
    build_mod(&plan, &profile, &output).unwrap();
    let gfx = fs::read_to_string(output.join("interface/fonts.gfx")).unwrap();
    assert!(!gfx.contains("bitmapfont_override"));
    for (group, dimensions, size) in [("hoi_16mbs", (16, 8), 32.0), ("jura", (12, 6), 24.0)] {
        let path = format!("gfx/fonts/ui_scaler/{group}/hoi");
        assert!(gfx.contains(&path));
        let atlas =
            textures::decode(&fs::read(output.join(format!("{path}.dds"))).unwrap()).unwrap();
        assert_eq!(atlas.dimensions(), dimensions);
        assert_eq!(atlas.get_pixel(0, 0).0, [30, 70, 120, 160]);
        let metrics = fs::read_to_string(output.join(format!("{path}.fnt"))).unwrap();
        let parsed = fonts::bitmap::Font::parse(&metrics).unwrap();
        assert_eq!(parsed.size, size);
        assert_eq!(parsed.dimensions, dimensions);
        assert!(metrics.contains("face=\"Original hoi\""));
        let resolved = plan
            .fonts
            .groups
            .iter()
            .find(|item| item.name == group)
            .unwrap()
            .resolve(
                "l_russian",
                &profile.fonts[group],
                profile.scale,
                FontMode::Automatic,
            );
        assert!(!resolved.ttf);
        assert_eq!(resolved.size, Some(size));
    }
    let metrics = fs::read_to_string(output.join("gfx/fonts/ui_scaler/hoi_16mbs/hoi.fnt")).unwrap();
    assert!(metrics.contains("lineHeight=32 base=26 scaleW=16 scaleH=8 pages=1"));
    assert!(
        metrics.contains("id=65 x=2 y=2 width=6 height=4 xoffset=-2 yoffset=0 xadvance=8 page=0")
    );
    assert!(metrics.contains("kerning first=65 second=66 amount=-2"));
    assert_eq!(fs::read(root.join("gfx/fonts/hoi.fnt")).unwrap(), original);
    profile.fonts.clear();
    build_mod(&plan, &profile, &output).unwrap();
    assert!(
        !output
            .join("gfx/fonts/ui_scaler/hoi_16mbs/hoi.fnt")
            .exists()
    );
    assert!(!output.join("gfx/fonts/ui_scaler/jura/hoi.dds").exists());
    assert!(
        fs::read_to_string(output.join("interface/fonts.gfx"))
            .unwrap()
            .contains("bitmapfont_override")
    );
}

#[test]
fn bitmap_glyph_edges_and_page_names_follow_atlas_dimensions() {
    let source = "info face=\"Same font\" size=16 padding=1,2,1,2 spacing=1,1\ncommon lineHeight=18 base=14 scaleW=8 scaleH=4 pages=1\npage id=0 file=\"old.tga\"\nchar id=65 x=1 y=0 width=3 height=4 xoffset=-1 yoffset=0 xadvance=4 page=0\nchar id=66 x=4 y=0 width=4 height=4 xoffset=0 yoffset=0 xadvance=4 page=0\n";
    for prefix in ["", "\u{feff}"] {
        let source = format!("{prefix}{source}");
        let parsed = fonts::bitmap::Font::parse(&source).unwrap();
        let (text, dimensions) = parsed.resize(25.0, "new.dds").unwrap();
        assert_eq!(dimensions, (12, 6));
        assert!(text.contains("face=\"Same font\" size=25"));
        assert!(text.contains("page id=0 file=\"new.dds\""));
        assert!(text.contains("char id=65 x=2 y=0 width=4 height=6"));
        assert!(text.contains("char id=66 x=6 y=0 width=6 height=6"));
        assert!(parsed.resize(f64::INFINITY, "invalid.dds").is_err());
        let (same, _) = parsed.resize(16.0, "old.tga").unwrap();
        assert_eq!(same, source);
    }
    assert!(fonts::bitmap::Font::parse(&source.replace("pages=1", "pages=2")).is_err());
}
