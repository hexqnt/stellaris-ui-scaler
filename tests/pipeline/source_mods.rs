use super::*;

#[test]
fn source_mod_layout_is_scaled_once_and_vanilla_has_no_fitted_offsets() {
    let fixture = fixture();
    let game = fixture.0.join("game");
    let localized = fixture.0.join("localization");
    let path = "interface/planet_view.gui";
    let vanilla = r"guiTypes = {
        containerWindowType = { name = planetary_resources size = { width = 277 height = 168 }
            instantTextBoxType = { name = planetary_production_output_title position = { x = 95 y = 7 } maxWidth = 274 }
        }
        containerWindowType = { name = zone_entry position = { x = 4 y = 0 } size = { width = 289 height = 50 }
            instantTextBoxType = { name = zone_title position = { x = 30 y = 0 } maxWidth = 100 }
            gridBoxType = { name = buildings_box position = { x = 134 y = 0 } slotSize = { width = 52 height = 50 } max_slots_horizontal = 6 }
        }
    }";
    let translated = vanilla
        .replace("x = 95", "x = 55")
        .replace("x = 4 y = 0", "x = -11 y = 0")
        .replace("width = 289", "width = 304")
        .replace("maxWidth = 100", "maxWidth = 115")
        .replace("x = 134", "x = 149");
    write(&game, path, vanilla);
    write(&localized, path, &translated);
    let control = Control::default();
    let output = fixture.0.join("mod");
    for source_mods in [Vec::new(), vec![localized]] {
        let plan = compiler::plan_with_mods(&game, &source_mods, &control, &mut |_| {}).unwrap();
        let input = if source_mods.is_empty() {
            vanilla
        } else {
            &translated
        };
        assert_eq!(plan.sources[path], input);
        for factor in [0.5, 1.0, 1.3846, 1.86921, 2.0, 3.0] {
            let scale = Scale::try_from(factor).unwrap();
            let profile = Profile {
                scale,
                source_mods: source_mods.clone(),
                ..Profile::default()
            };
            compiler::build(&plan, &profile, &output, &control, &mut |_| {}).unwrap();
            let gui = fs::read_to_string(output.join(path))
                .unwrap_or_else(|_| plan.game.text(path).unwrap());
            assert_eq!(
                gui,
                scaling::geometry(input, scale, false, path, &mut Vec::new()).unwrap()
            );
            let doc = Document::parse(&gui).unwrap();
            let zone = named(&doc.nodes[0], "zone_entry");
            let buildings = named(zone, "buildings_box");
            // Локализация меняет исходное начало зоны и возвращает здания на прежнее место.
            assert!(
                (number(zone, "position", "x") + number(buildings, "position", "x")
                    - scale.pixels(138.0))
                .abs()
                    <= 1.0
            );
            assert_eq!(number(buildings, "slotSize", "width"), scale.pixels(52.0));
        }
    }
}

#[test]
fn source_mod_priority_applies_before_sprite_texture_and_font_analysis() {
    let fixture = fixture();
    let game = fixture.0.join("game");
    let first = fixture.0.join("first");
    let last = fixture.0.join("last");
    write(
        &first,
        "interface/example.gfx",
        r#"spriteTypes = { spriteType = { name = GFX_mod textureFile = "gfx/mod.png" noOfFrames = 3 } }"#,
    );
    write(
        &first,
        "interface/example.gui",
        r"guiTypes = { iconType = { name = mod_icon spriteType = GFX_mod position = { x = 5 y = 6 } } }",
    );
    write(
        &first,
        "interface/mod_fonts.gfx",
        r"bitmapfonts = { bitmapfont = { name = mod_font } bitmapfont_override = { name = mod_font ttf_font = ModFont ttf_size = 14 languages = { l_russian } } }",
    );
    write(&first, "fonts/mod_font.asset", "font = { name = ModFont }");
    write(&first, "common/ignored.txt", "gameplay data");
    fs::create_dir_all(first.join("gfx")).unwrap();
    fs::create_dir_all(last.join("gfx")).unwrap();
    image::RgbaImage::from_pixel(12, 4, image::Rgba([255, 0, 0, 255]))
        .save(first.join("gfx/mod.png"))
        .unwrap();
    image::RgbaImage::from_pixel(18, 5, image::Rgba([0, 255, 0, 255]))
        .save(last.join("gfx/MOD.PNG"))
        .unwrap();
    let control = Control::default();
    let source_mods = vec![first, last];
    let plan = compiler::plan_with_mods(&game, &source_mods, &control, &mut |_| {}).unwrap();
    assert!(!plan.game.contains("common/ignored.txt"));
    assert!(plan.fonts.families.iter().any(|family| family == "ModFont"));
    assert_eq!(
        plan.fonts
            .groups
            .iter()
            .find(|group| group.name == "mod_font")
            .unwrap()
            .native("l_russian")
            .unwrap()
            .size,
        14.0
    );
    let texture = plan
        .textures
        .iter()
        .find(|texture| texture.path == "gfx/mod.png")
        .unwrap();
    assert_eq!((texture.width, texture.height, texture.frames), (18, 5, 3));
    let output = fixture.0.join("mod");
    let profile = Profile {
        scale: Scale::try_from(2.0).unwrap(),
        source_mods,
        ..Profile::default()
    };
    let report = compiler::build(&plan, &profile, &output, &control, &mut |_| {}).unwrap();
    assert!(report.validation.errors.is_empty());
    let image = image::open(output.join("gfx/mod_scaled.png"))
        .unwrap()
        .to_rgba8();
    assert_eq!(image.dimensions(), (36, 10));
    assert_eq!(image.get_pixel(1, 1).0, [0, 255, 0, 255]);
    let gui = fs::read_to_string(output.join("interface/example.gui")).unwrap();
    assert!(!gui.contains("scale ="));
    let fonts = fs::read_to_string(output.join("interface/mod_fonts.gfx")).unwrap();
    assert!(fonts.contains("ttf_size = 28"));
    assert!(!output.join("common").exists());
}

#[test]
fn source_changes_require_rescan_and_outputs_cannot_overwrite_source_mods() {
    let fixture = fixture();
    let game = fixture.0.join("game");
    let source_mod = fixture.0.join("source");
    write(
        &source_mod,
        "interface/example.gui",
        "containerWindowType = { name = source size = { width = 10 height = 10 } }",
    );
    let control = Control::default();
    let source_mods = vec![source_mod.clone()];
    let plan = compiler::plan_with_mods(&game, &source_mods, &control, &mut |_| {}).unwrap();
    assert!(
        compiler::build(
            &plan,
            &Profile::default(),
            &fixture.0.join("output"),
            &control,
            &mut |_| {}
        )
        .is_err()
    );
    let profile = Profile {
        source_mods,
        ..Profile::default()
    };
    let saved = fixture.0.join("profile.toml");
    profile.save(&saved).unwrap();
    assert_eq!(
        Profile::load(&saved).unwrap().source_mods,
        profile.source_mods
    );
    for output in [&source_mod, &source_mod.join("output")] {
        assert!(compiler::build(&plan, &profile, output, &control, &mut |_| {}).is_err());
    }
    assert!(!source_mod.join("output").exists());
    assert!(
        compiler::plan_with_mods(&game, &[fixture.0.join("missing")], &control, &mut |_| {})
            .is_err()
    );
}
