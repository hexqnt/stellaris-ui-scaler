use super::*;

#[test]
fn combined_gui_pass_preserves_sequential_transformations_and_diagnostic_spans() {
    let fixture = fixture();
    let game = fixture.0.join("game");
    let source = r#"guiTypes = {
        containerWindowType = { size = { width = "17" height = @[ height + 2 ] }
            instantTextBoxType = { name = label maxwidth = "23" }
            instantTextBoxType = { name = implicit_width }
            iconType = { spriteType = GFX_portrait scale = 0.5 }
            iconType = { spriteType = GFX_portrait scale = @dynamic }
        }
    }"#;
    write(&game, "interface/example.gui", source);
    write(
        &game,
        "interface/portrait.gfx",
        "portraitType = { name = GFX_portrait }",
    );
    let control = Control::default();
    let plan = compiler::plan(&game, &control, &mut |_| {}).unwrap();
    for factor in [0.5, 1.0, 1.3846, 2.0] {
        let profile = Profile {
            scale: Scale::try_from(factor).unwrap(),
            ..Default::default()
        };
        let mut diagnostics = Vec::new();
        let expected = scaling::geometry(
            source,
            profile.scale,
            false,
            "interface/example.gui",
            &mut diagnostics,
        )
        .unwrap();
        let expected = scaling::widgets(
            &expected,
            profile.scale,
            &BTreeMap::from([("GFX_portrait".into(), false)]),
        )
        .unwrap();
        let output = fixture.0.join("mod");
        let report = compiler::build(&plan, &profile, &output, &control, &mut |_| {}).unwrap();
        assert_eq!(
            fs::read_to_string(output.join("interface/example.gui")).unwrap(),
            expected
        );
        assert_eq!(
            serde_json::to_value(&report.diagnostics).unwrap(),
            serde_json::to_value(&diagnostics).unwrap()
        );
    }
}

#[test]
fn combined_gfx_pass_preserves_nested_references_and_expression_spans() {
    let fixture = fixture();
    let game = fixture.0.join("game");
    let control = Control::default();
    for value in ["7", "@[ height + 2 ]"] {
        let source = format!(
            r#"spriteTypes = {{ spriteType = {{ name = GFX_test texturefile = "gfx/test.png" noOfFrames = 2 animation = {{ animationmaskfile = "gfx/test.png" }} size = {{ x = "3" y = {value} }} }} }}"#
        );
        write(&game, "interface/example.gfx", &source);
        let plan = compiler::plan(&game, &control, &mut |_| {}).unwrap();
        let profile = Profile::default();
        let mut diagnostics = Vec::new();
        let expected = source.replace("gfx/test.png", "gfx/test_scaled.png");
        let expected = scaling::geometry(
            &expected,
            profile.scale,
            true,
            "interface/example.gfx",
            &mut diagnostics,
        )
        .unwrap();
        let output = fixture.0.join("mod");
        let report = compiler::build(&plan, &profile, &output, &control, &mut |_| {}).unwrap();
        assert_eq!(
            fs::read_to_string(output.join("interface/example.gfx")).unwrap(),
            expected
        );
        assert_eq!(
            serde_json::to_value(&report.diagnostics).unwrap(),
            serde_json::to_value(&diagnostics).unwrap()
        );
    }
}

#[test]
fn button_text_offsets_follow_gui_scale() {
    let source = r#"guiTypes = {
        buttonType = {
            name = "general_settings_tab"
            quadTextureSprite = "GFX_tab_gamesetup"
            position = { x = 620 y = 60 }
            scale = 0.75
            font = "malgun_goth_24"
            buttontext = "GALAXY_SETTINGS_GENERAL"
            text_offset = { x = -30 y = -12 }
        }
        buttonType = {
            name = "advanced_settings_tab"
            buttontext = "GALAXY_SETTINGS_ADVANCED"
            Text_Offset = { x = -30 y = -12 }
        }
        buttonType = {
            name = "custom_tab"
            text_offset = { x = 10 y = 0 }
        }
    }"#;
    for factor in [0.5, 1.0, 1.3846, 1.86921, 2.0] {
        let scale = Scale::try_from(factor).unwrap();
        for path in [
            "interface/game_setup/setup.gui",
            "interface/mp_gamesetup.gui",
        ] {
            let result = scaling::geometry(source, scale, false, path, &mut Vec::new()).unwrap();
            let doc = Document::parse(&result).unwrap();
            let buttons = doc.nodes[0].children();
            for button in &buttons[..2] {
                assert_eq!(number(button, "text_offset", "x"), scale.pixels(-30.0));
                assert_eq!(number(button, "text_offset", "y"), scale.pixels(-12.0));
            }
            assert_eq!(number(&buttons[2], "text_offset", "x"), scale.pixels(10.0));
            assert_eq!(number(&buttons[2], "text_offset", "y"), 0.0);
            assert_eq!(buttons[0].field("scale").unwrap().atom(), Some("0.75"));
            assert_eq!(
                buttons[0].field("font").unwrap().atom(),
                Some("malgun_goth_24")
            );
            assert_eq!(
                buttons[0].field("buttontext").unwrap().atom(),
                Some("GALAXY_SETTINGS_GENERAL")
            );
        }
    }
}

#[test]
fn geometry_is_contextual_and_rounds_like_python() {
    let source = "guiTypes = { @s = 3 iconType = { name = \"test\" position { x = -3 y = 0 } size = { width = 100% height = @s } scale = 0.8 } if_scaled_resolution = { min_width = 1920 containerWindowType = { maxWidth = 200 } } }";
    let result = scaling::geometry(
        source,
        Scale::try_from(1.5).unwrap(),
        false,
        "test.gui",
        &mut Vec::new(),
    )
    .unwrap();
    assert!(result.contains("@s = 4"));
    assert!(result.contains("x = -4"));
    assert!(result.contains("width = 100%"));
    assert!(result.contains("min_width = 1920"));
    assert!(result.contains("maxWidth = 300"));
    assert!(result.contains("scale = 0.8"));
}

#[test]
fn compact_rows_keep_engine_sprite_names() {
    let source = "guiTypes = { containerWindowType = { name = \"outliner_member_fleet_entry_window\" size = { width = 353 height = 69 } buttonType = { name = \"background\" quadTextureSprite = \"GFX_outliner_member_bg\" } instantTextBoxType = { name = \"name\" position = { x = 40 y = 3 } } } }";
    let profile = Profile::default();
    let result = outliner::transform(source, &profile.outliner, profile.scale).unwrap();
    assert!(result.contains("GFX_outliner_member_bg"));
    assert!(result.contains("cg_16b"));
    Document::parse(&result).unwrap();
}

#[test]
fn widgets_only_scale_render_targets_without_explicit_size() {
    let source = "iconType = { name = \"portrait\" spriteType = \"GFX_portrait\" } iconType = { size = { x = 10 y = 10 } spriteType = \"GFX_portrait\" }";
    let result = scaling::widgets(
        source,
        Scale::try_from(2.0).unwrap(),
        &BTreeMap::from([("GFX_portrait".into(), false)]),
    )
    .unwrap();
    assert_eq!(result.matches("scale = 2.0000").count(), 1);
}

#[test]
fn runtime_sprite_replacements_preserve_original_widget_scale() {
    let source = r#"guiTypes = {
        iconType = { name = "large_resource_icon" spriteType = "gfx_transparency" }
        iconType = { name = "astral_action_unlocked" quadTextureSprite = "GFX_tiled_window_transparent" }
        iconType = { name = "specimen" quadTextureSprite = "GFX_invisible" scale = 0.6 }
        iconType = { name = "diplomacy_type_icon" quadTextureSprite = "GFX_diplo_type_icon" }
        iconType = { name = "pact_icon" quadTextureSprite = "GFX_diplo_type_icon" scale = 0.9 }
        iconType = { name = "static_tile" quadTextureSprite = "GFX_tiled_window_transparent" }
        iconType = { name = "portrait" spriteType = "GFX_portrait" }
    }"#;
    let statuses = BTreeMap::from([
        ("gfx_transparency".into(), false),
        ("GFX_tiled_window_transparent".into(), false),
        ("GFX_invisible".into(), false),
        ("GFX_diplo_type_icon".into(), false),
        ("GFX_portrait".into(), false),
    ]);
    for factor in [1.0, 1.3846, 1.86921, 2.0] {
        let result = scaling::widgets(source, Scale::try_from(factor).unwrap(), &statuses).unwrap();
        let doc = Document::parse(&result).unwrap();
        let icons = doc.nodes[0].children();
        for index in [0, 1, 3] {
            assert!(icons[index].field("scale").is_none());
        }
        assert_eq!(icons[2].field("scale").unwrap().atom(), Some("0.6"));
        assert_eq!(icons[4].field("scale").unwrap().atom(), Some("0.9"));
        for icon in &icons[5..] {
            assert_eq!(
                icon.field("scale").unwrap().atom().unwrap(),
                format!("{factor:.4}")
            );
        }
    }
}

#[test]
fn dynamic_icon_families_keep_scale_but_static_placeholders_still_scale() {
    let invisible = [
        "situation_icon",
        "situation_icon_background",
        "situation_entry_icon",
        "situation_entry_icon_background",
        "approach_icon",
        "approach_icon_bg",
        "stage_icon",
        "stage_icon_background",
        "outcome_left",
        "outcome_left_frame",
        "outcome_right",
        "outcome_right_frame",
        "planet_type_icon_overlay",
        "designation_icon_overlay",
        "specimen_icon",
        "leader_rarity_frame",
        "covenant_icon",
        "active_accord_button",
        "not_eligible",
        "portrait_overlay",
        "dlc_button",
        "dlc_frame",
        "dlc_underlay",
    ];
    let mut source = String::from("guiTypes = {");
    for name in invisible {
        write!(
            source,
            "iconType = {{ name = {name} spriteType = GFX_invisible scale = 0.70 }}"
        )
        .unwrap();
    }
    source.push_str(r"
        iconType = { name = relic_portrait quadTextureSprite = GFX_tiled_window_transparent }
        buttonType = { name = passive_accord_icon spriteType = GFX_invisible_transparent scale = 0.5 }
        containerWindowType = { name = specimen_acquired_custom_container
            iconType = { name = frame spriteType = GFX_invisible }
        }
        containerWindowType = { name = timeline_grid_item_origin_zoom_1
            iconType = { name = background spriteType = GFX_pink_background scale = 1.12 }
        }
    }");
    let statuses = [
        "GFX_invisible",
        "GFX_invisible_transparent",
        "GFX_tiled_window_transparent",
        "GFX_pink_background",
    ]
    .map(|name| (name.into(), false))
    .into();
    let controls = r"guiTypes = {
        iconType = { name = static_icon spriteType = GFX_invisible }
        buttonType = { name = static_button spriteType = GFX_invisible_transparent }
        iconType = { name = frame spriteType = GFX_invisible }
        iconType = { name = background spriteType = GFX_pink_background }
        iconType = { name = situation_icon spriteType = GFX_pink_background }
        containerWindowType = { name = unrelated
            iconType = { name = frame spriteType = GFX_invisible }
            iconType = { name = background spriteType = GFX_pink_background }
        }
    }";
    for factor in [1.0, 1.3846, 1.86921, 2.0] {
        let scale = Scale::try_from(factor).unwrap();
        assert_eq!(scaling::widgets(&source, scale, &statuses).unwrap(), source);
        let result = scaling::widgets(controls, scale, &statuses).unwrap();
        assert_eq!(result.matches(&format!("scale = {factor:.4}")).count(), 7);
    }
}

#[test]
fn situation_icons_scale_once_in_standard_and_compact_builds() {
    let fixture = fixture();
    let root = fixture.0.join("game");
    write(
        &root,
        "interface/outliner.gui",
        r"guiTypes = { containerWindowType = {
            name = outliner_member_situations_entry_window
            size = { width = 318 height = 40 }
            iconType = { name = situation_icon_background spriteType = GFX_invisible position = { x = 21 y = 21 } }
            iconType = { name = situation_icon spriteType = GFX_invisible position = { x = 21 y = 21 } }
        } }",
    );
    write(
        &root,
        "interface/situation_log.gfx",
        r#"spriteTypes = {
            corneredTileSpriteType = { name = GFX_invisible textureFile = "gfx/test.png" borderSize = { x = 1 y = 1 } }
            spriteType = { name = GFX_situation_stage textureFile = "gfx/situation.png" }
        }"#,
    );
    image::RgbaImage::from_pixel(32, 32, image::Rgba([0, 255, 0, 255]))
        .save(root.join("gfx/situation.png"))
        .unwrap();
    write(
        &root,
        "interface/outliner.gfx",
        r#"spriteTypes = { spriteType = { name = GFX_outliner_member_bg textureFile = "gfx/test.png" } }"#,
    );
    let backgrounds = root.join("gfx/interface/buttons");
    fs::create_dir_all(&backgrounds).unwrap();
    let background = image::RgbaImage::from_pixel(96, 32, image::Rgba([0, 128, 0, 255]));
    let dds = textures::resource_dds(&background).unwrap();
    for name in ["outliner_entry.dds", "outliner_selected_entry.dds"] {
        fs::write(backgrounds.join(name), &dds).unwrap();
    }
    let control = Control::default();
    let plan = compiler::plan(&root, &control, &mut |_| {}).unwrap();
    let output = fixture.0.join("mod");
    for factor in [1.3846, 1.86921, 2.0] {
        let scale = Scale::try_from(factor).unwrap();
        for compact in [false, true] {
            let mut profile = Profile {
                scale,
                ..Profile::default()
            };
            profile.outliner.compact = compact;
            compiler::build(&plan, &profile, &output, &control, &mut |_| {}).unwrap();
            let text = fs::read_to_string(output.join("interface/outliner.gui")).unwrap();
            let doc = Document::parse(&text).unwrap();
            let icons = doc.nodes[0].children()[0].children();
            for icon in icons.iter().filter(|node| node.key == Some("iconType")) {
                assert!(icon.field("scale").is_none(), "{factor}, compact={compact}");
                let position = icon.field("position").unwrap();
                assert_eq!(
                    position.field("x").unwrap().parse_atom::<f64>(),
                    Some(scale.pixels(21.0))
                );
            }
            let texture = image::open(output.join("gfx/situation_scaled.png")).unwrap();
            assert_eq!(texture.width(), scale.dimension(32));
            assert_eq!(texture.height(), scale.dimension(32));
            let gfx = fs::read_to_string(output.join("interface/situation_log.gfx")).unwrap();
            assert!(gfx.contains("gfx/situation_scaled.png"));
        }
    }
}

#[test]
fn procedural_sprite_dimensions_prevent_a_second_widget_scale() {
    let fixture = fixture();
    let root = fixture.0.join("game");
    write(
        &root,
        "interface/charts.gfx",
        r"spriteTypes = {
        PieChartType = { name = GFX_chart size = 50 }
        spriteType = { name = GFX_chart_alias sprite_sheet_sprite_type = GFX_chart }
        PieChartType = { name = GFX_unknown_chart size = @unknown }
    }",
    );
    write(
        &root,
        "interface/charts.gui",
        r"guiTypes = {
        iconType = { name = chart spriteType = GFX_chart position = { x = 10 y = 20 } }
        iconType = { name = small_chart spriteType = GFX_chart_alias scale = 0.5 }
        iconType = { name = unknown_chart spriteType = GFX_unknown_chart }
    }",
    );
    let control = Control::default();
    let plan = compiler::plan(&root, &control, &mut |_| {}).unwrap();
    let output = fixture.0.join("mod");
    for factor in [1.3846, 1.86921, 2.0] {
        let scale = Scale::try_from(factor).unwrap();
        let profile = Profile {
            scale,
            ..Profile::default()
        };
        compiler::build(&plan, &profile, &output, &control, &mut |_| {}).unwrap();
        let gui = fs::read_to_string(output.join("interface/charts.gui")).unwrap();
        let doc = Document::parse(&gui).unwrap();
        let icons = doc.nodes[0].children();
        assert!(icons[0].field("scale").is_none());
        assert_eq!(icons[1].field("scale").unwrap().atom(), Some("0.5"));
        assert_eq!(
            icons[2].field("scale").unwrap().atom().unwrap(),
            format!("{factor:.4}")
        );
        let gfx = fs::read_to_string(output.join("interface/charts.gfx")).unwrap();
        let doc = Document::parse(&gfx).unwrap();
        assert_eq!(
            doc.nodes[0].children()[0]
                .field("size")
                .unwrap()
                .parse_atom::<f64>(),
            Some(scale.pixels(50.0))
        );
    }
}

#[test]
fn ship_designer_slot_counts_are_not_pixel_coordinates() {
    let source = r#"guiTypes = {
        positionType = { name = "min_component_slots" position = { x = 3 y = 3 } }
        positionType = { name = "required_slots_position" position = { x = 945 y = 374 } }
        gridBoxType = { name = "weapon_slots" slotSize = { width = 56 height = 56 } max_slots_horizontal = 3 }
    }"#;
    for factor in [1.0, 1.3846, 1.86921, 2.0] {
        let scale = Scale::try_from(factor).unwrap();
        let result = scaling::geometry(
            source,
            scale,
            false,
            "interface/ship_designer.gui",
            &mut Vec::new(),
        )
        .unwrap();
        let doc = Document::parse(&result).unwrap();
        let nodes = doc.nodes[0].children();
        let counts = nodes[0].field("position").unwrap();
        assert_eq!(counts.field("x").unwrap().atom(), Some("3"));
        assert_eq!(counts.field("y").unwrap().atom(), Some("3"));
        let position = nodes[1].field("position").unwrap();
        assert_eq!(
            position.field("x").unwrap().atom().unwrap(),
            format!("{:.0}", scale.pixels(945.0))
        );
        assert_eq!(
            position.field("y").unwrap().atom().unwrap(),
            format!("{:.0}", scale.pixels(374.0))
        );
        assert_eq!(
            nodes[2]
                .field("slotSize")
                .unwrap()
                .field("width")
                .unwrap()
                .atom()
                .unwrap(),
            format!("{:.0}", scale.pixels(56.0))
        );
        assert_eq!(
            nodes[2].field("max_slots_horizontal").unwrap().atom(),
            Some("3")
        );
    }
}

#[test]
fn compact_toasts_follow_configured_panel_width() {
    let source = "guiTypes = { containerWindowType = { name = \"toast_container\" show_position = { x = -768 y = 100 } hide_position = { x = -768 y = -100 } } }";
    let options = stellaris_ui_scaler::config::OutlinerOptions {
        compact: true,
        width: 800,
        ..Default::default()
    };
    let text = outliner::shared_gui(
        source,
        "interface/alerts.gui",
        Scale::try_from(1.3846).unwrap(),
        &options,
        &mut Vec::new(),
    )
    .unwrap();
    assert_eq!(text.matches("x = -1250").count(), 2);
    assert!(text.contains("y = -100"));
}

#[test]
fn standard_layout_has_no_reference_mod_overrides() {
    let fixture = fixture();
    let root = fixture.0.join("game");
    let cases = [
        (
            "main.gui",
            "music_controls_toggle",
            "position = { x = 5 y = 11 }",
            "position = { x = 10 y = 22 }",
        ),
        (
            "main.gui",
            "music_commands_win",
            "position = { x = 5 y = 13 }",
            "position = { x = 10 y = 26 }",
        ),
        (
            "ship_designer.gui",
            "sections",
            "spacing = 17",
            "spacing = 34",
        ),
        (
            "topbar_species_view.gui",
            "set_rights_or_policy_button",
            "position = { x = 7 y = 11 }",
            "position = { x = 14 y = 22 }",
        ),
        (
            "topbar_species_view.gui",
            "set_default",
            "position = { x = 9 y = 11 }",
            "position = { x = 18 y = 22 }",
        ),
        (
            "topbar_species_view.gui",
            "modify_button",
            "position = { x = 19 y = 11 }",
            "position = { x = 38 y = 22 }",
        ),
        (
            "topbar_species_view.gui",
            "apply_template_or_uplift_button",
            "position = { x = 21 y = 11 }",
            "position = { x = 42 y = 22 }",
        ),
        (
            "agreement_negotiation_view.gui",
            "target_country_name",
            "position = { x = 3 y = 15 } font = other_font",
            "position = { x = 6 y = 30 } font = other_font",
        ),
        (
            "alerts.gui",
            "toast_container",
            "show_position = { x = -30 y = 15 } hide_position = { x = -30 y = -15 }",
            "show_position = { x = -60 y = 30 } hide_position = { x = -60 y = -30 }",
        ),
    ];
    let mut sources = BTreeMap::<&str, String>::new();
    for (file, name, properties, _) in cases {
        use std::fmt::Write;
        writeln!(
            sources.entry(file).or_default(),
            "containerWindowType = {{ name = \"{name}\" {properties} }}"
        )
        .unwrap();
    }
    for (file, source) in sources {
        write(&root, &format!("interface/{file}"), &source);
    }
    let plan = compiler::plan(&root, &Control::default(), &mut |_| {}).unwrap();
    let profile = Profile {
        scale: Scale::try_from(2.0).unwrap(),
        ..Profile::default()
    };
    let output = fixture.0.join("mod");
    compiler::build(&plan, &profile, &output, &Control::default(), &mut |_| {}).unwrap();
    for (file, _, _, expected) in cases {
        let gui = fs::read_to_string(output.join(format!("interface/{file}"))).unwrap();
        assert!(gui.contains(expected), "{file}: {expected}");
    }
}

#[test]
fn sprite_sheet_aliases_inherit_texture_scaling() {
    let fixture = fixture();
    let root = fixture.0.join("game");
    write(
        &root,
        "interface/aliases.gfx",
        r#"spriteTypes = {
        spriteType = { name = "GFX_chain" sprite_sheet_sprite_type = "GFX_upgrade" }
        spriteType = { name = "GFX_upgrade" sprite_sheet_sprite_type = GFX_test default_frame = 2 }
        spriteType = { name = "GFX_cycle_a" sprite_sheet_sprite_type = GFX_cycle_b }
        spriteType = { name = "GFX_cycle_b" sprite_sheet_sprite_type = GFX_cycle_a }
        spriteType = { name = "GFX_missing" sprite_sheet_sprite_type = GFX_absent }
        corneredTileSpriteType = { name = "GFX_tile" textureFile = "gfx/tile.png" borderSize = { x = 1 y = 1 } }
        spriteType = { name = "GFX_tile_alias" sprite_sheet_sprite_type = GFX_tile }
    }"#,
    );
    fs::copy(root.join("gfx/test.png"), root.join("gfx/tile.png")).unwrap();
    write(
        &root,
        "interface/aliases.gui",
        r#"guiTypes = {
        iconType = { name = "upgrade" spriteType = "GFX_upgrade" position = { x = 223 y = 21 } }
        iconType = { name = "chain" spriteType = "GFX_chain" scale = 0.5 }
        iconType = { name = "tile" spriteType = "GFX_tile_alias" }
        iconType = { name = "cycle" spriteType = "GFX_cycle_a" }
        iconType = { name = "missing" spriteType = "GFX_missing" }
    }"#,
    );
    let plan = compiler::plan(&root, &Control::default(), &mut |_| {}).unwrap();
    let output = fixture.0.join("mod");
    for factor in [1.3846, 2.0] {
        let scale = Scale::try_from(factor).unwrap();
        let profile = Profile {
            scale,
            ..Profile::default()
        };
        compiler::build(&plan, &profile, &output, &Control::default(), &mut |_| {}).unwrap();
        let gui = fs::read_to_string(output.join("interface/aliases.gui")).unwrap();
        let doc = Document::parse(&gui).unwrap();
        let icons = doc.nodes[0].children();
        assert!(icons[0].field("scale").is_none());
        assert_eq!(icons[1].field("scale").unwrap().atom(), Some("0.5"));
        for icon in &icons[2..] {
            assert_eq!(
                icon.field("scale")
                    .unwrap()
                    .atom()
                    .unwrap()
                    .parse::<f64>()
                    .unwrap(),
                (factor * 10000.0).round() / 10000.0
            );
        }
        let atlas = image::open(output.join(textures::output_name("gfx/test.png"))).unwrap();
        assert_eq!(atlas.width(), scale.dimension(4) * 2);
        assert_eq!(atlas.height(), scale.dimension(4));
    }
}
