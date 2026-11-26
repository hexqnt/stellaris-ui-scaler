use super::*;

#[test]
fn trade_view_wide_mode_scales_engine_width_and_resolution_threshold() {
    let fixture = fixture();
    let root = fixture.0.join("game");
    let gui_path = "interface/trade_view.gui";
    let defines_path = "unchecked_defines/00_interface.txt";
    let source = r#"guiTypes = {
        positionType = { name = "trade_view_wide_mode_demands_pos" position = { x = 1220 y = 10 } }
        containerWindowType = { name = "trade_view" size = { width = 1232 height = 640 }
            containerWindowType = { name = "diplo_trade_view_background_window" size = { width = 1232 height = 640 } }
            containerWindowType = { name = "demands_trade_categories_window" position = { x = 11 y = 11 } size = { width = 240 height = 605 } }
            containerWindowType = { name = "demands_trade_offer_window" position = { x = 852 y = 460 } size = { width = 360 height = 156 } }
        }
    }"#;
    let defines = r"NInterface = {
        TRADE_VIEW_WIDE_MODE_THRESHHOLD = 1470
        TRADE_VIEW_SMALL_CREDIT_CHANGE = 10
        TRADE_VIEW_LARGE_CREDIT_CHANGE = 100
    }";
    write(&root, gui_path, source);
    write(&root, defines_path, defines);
    let plan = scan_game(&root).unwrap();
    let output = fixture.0.join("mod");
    for factor in [0.5, 1.0, 1.3846, 1.86921, 2.0] {
        let scale = Scale::try_from(factor).unwrap();
        let profile = Profile {
            scale,
            ..Profile::default()
        };
        build_mod(&plan, &profile, &output).unwrap();
        let gui = fs::read_to_string(output.join(gui_path)).unwrap_or_else(|_| source.into());
        let built_defines =
            fs::read_to_string(output.join(defines_path)).unwrap_or_else(|_| defines.into());
        let doc = Document::parse(&gui).unwrap();
        let defines = Document::parse(&built_defines).unwrap();
        let interface = &defines.nodes[0];
        let threshold = interface
            .field("TRADE_VIEW_WIDE_MODE_THRESHHOLD")
            .unwrap()
            .parse_atom::<f64>()
            .unwrap();
        assert_eq!(threshold, scale.pixels(1470.0));
        assert_eq!(
            interface
                .field("TRADE_VIEW_SMALL_CREDIT_CHANGE")
                .unwrap()
                .atom(),
            Some("10")
        );
        assert_eq!(
            interface
                .field("TRADE_VIEW_LARGE_CREDIT_CHANGE")
                .unwrap()
                .atom(),
            Some("100")
        );
        let nodes = doc.nodes[0].children();
        let wide_position = number(&nodes[0], "position", "x");
        let window = &nodes[1];
        let children = window.children();
        let background = &children[2];
        let categories = &children[3];
        let offers = &children[4];
        assert_eq!(number(window, "size", "width"), scale.pixels(1232.0));
        assert_eq!(wide_position, scale.pixels(1220.0));
        // Setup заменяет ширину окна и фона константой только при resolution > threshold.
        for resolution in [threshold - 1.0, threshold, threshold + 1.0, threshold * 2.0] {
            let wide = resolution > threshold;
            let width = if wide {
                threshold
            } else {
                number(window, "size", "width")
            };
            let background_width = if wide {
                threshold
            } else {
                number(background, "size", "width")
            };
            let categories_x = if wide {
                wide_position
            } else {
                number(categories, "position", "x")
            };
            assert_eq!(width, background_width);
            assert!(categories_x + number(categories, "size", "width") <= width);
            assert!(number(offers, "position", "x") + number(offers, "size", "width") <= width);
        }
    }
}

#[test]
fn music_panel_background_scales_dds_with_slice_borders() {
    let fixture = fixture();
    let root = fixture.0.join("game");
    let path = "gfx/interface/main/topbar_background_cutoff.dds";
    let source = image::RgbaImage::from_fn(62, 62, |_, y| {
        image::Rgba([20, 40, 30, if y < 30 { 255 } else { 0 }])
    });
    fs::create_dir_all(root.join("gfx/interface/main")).unwrap();
    fs::write(root.join(path), textures::resource_dds(&source).unwrap()).unwrap();
    write(
        &root,
        "interface/musicplayer.gfx",
        r#"spriteTypes = { corneredTileSpriteType = {
            name = "GFX_topbar_background_cutoff"
            textureFile = "gfx/interface/main/topbar_background_cutoff.dds"
            borderSize = { x = 30 y = 30 }
        } }"#,
    );
    write(
        &root,
        "interface/main.gui",
        r#"guiTypes = { containerWindowType = {
            name = "maingui_lower_topbar"
            size = { width = 170 height = 36 }
            background = { name = "background" quadTextureSprite = "GFX_topbar_background_cutoff" }
        } }"#,
    );
    let plan = scan_game(&root).unwrap();
    let output = fixture.0.join("mod");
    for factor in [1.0, 1.3846, 1.86921, 2.0] {
        let scale = Scale::try_from(factor).unwrap();
        let profile = Profile {
            scale,
            ..Profile::default()
        };
        build_mod(&plan, &profile, &output).unwrap();
        let bytes = fs::read(output.join(path)).unwrap();
        assert!(bytes.starts_with(b"DDS "));
        let image = textures::decode(&bytes).unwrap();
        assert_eq!(
            image.dimensions(),
            (scale.dimension(62), scale.dimension(62))
        );
        let x = scale.dimension(31);
        assert_eq!(image.get_pixel(x, scale.dimension(20))[3], 255);
        assert_eq!(image.get_pixel(x, scale.dimension(40))[3], 0);
        let gfx =
            fs::read_to_string(output.join("interface/musicplayer.gfx")).unwrap_or_else(|_| {
                fs::read_to_string(root.join("interface/musicplayer.gfx")).unwrap()
            });
        let doc = Document::parse(&gfx).unwrap();
        let sprite = &doc.nodes[0].children()[0];
        assert_eq!(sprite.field("textureFile").unwrap().atom(), Some(path));
        assert_eq!(
            sprite
                .field("borderSize")
                .unwrap()
                .field("y")
                .unwrap()
                .atom()
                .unwrap(),
            format!("{:.0}", scale.pixels(30.0))
        );
        let gui = fs::read_to_string(output.join("interface/main.gui"))
            .unwrap_or_else(|_| fs::read_to_string(root.join("interface/main.gui")).unwrap());
        let doc = Document::parse(&gui).unwrap();
        assert_eq!(
            doc.nodes[0].children()[0]
                .field("size")
                .unwrap()
                .field("height")
                .unwrap()
                .atom()
                .unwrap(),
            format!("{:.0}", scale.pixels(36.0))
        );
    }
    assert_eq!(
        textures::decode(&fs::read(root.join(path)).unwrap()).unwrap(),
        source
    );
}

#[test]
fn navbar_background_compensates_fixed_engine_position() {
    let fixture = fixture();
    let root = fixture.0.join("game");
    let path = "gfx/interface/main/navbar_bg_wide.dds";
    let image = image::RgbaImage::from_pixel(32, 61, image::Rgba([30, 70, 120, 255]));
    fs::create_dir_all(root.join("gfx/interface/main")).unwrap();
    fs::write(root.join(path), textures::resource_dds(&image).unwrap()).unwrap();
    write(
        &root,
        "interface/main.gfx",
        r#"spriteTypes = { spriteType = { name = "GFX_navbar_bg_wide" textureFile = "gfx/interface/main/navbar_bg_wide.dds" } }"#,
    );
    write(
        &root,
        "interface/main.gui",
        r#"containerWindowType = { name = "maingui_left_bar_buttons" position = { x = -225 y = 35 } size = { width = 250 height = 690 } containerWindowType = { name = "background" position = { x = 5 y = 5 } size = { width = 320 height = 610 } background = { name = "navbar_bg" spriteType = "GFX_navbar_bg_wide" } } }"#,
    );
    let plan = scan_game(&root).unwrap();
    let output = fixture.0.join("mod");
    for factor in [0.5, 1.0, 1.3846, 2.0] {
        let scale = Scale::try_from(factor).unwrap();
        let profile = Profile {
            scale,
            ..Profile::default()
        };
        build_mod(&plan, &profile, &output).unwrap();
        let built = image::open(output.join(textures::output_name(path))).unwrap();
        assert_eq!(built.width(), scale.dimension(32));
        assert_eq!(built.height(), scale.dimension(61));
        let gui = fs::read_to_string(output.join("interface/main.gui")).unwrap();
        let doc = Document::parse(&gui).unwrap();
        let panel = &doc.nodes[0];
        let background = panel.field("containerWindowType").unwrap();
        let compensation = background.field("containerWindowType").unwrap();
        assert_eq!(compensation.named(), Some("ui_scaler_navbar_background"));
        assert_eq!(
            compensation
                .field("size")
                .unwrap()
                .field("width")
                .unwrap()
                .atom(),
            Some("100%")
        );
        let dx = number(compensation, "position", "x");
        let dy = number(compensation, "position", "y");
        let width = number(background, "size", "width");
        let height = number(background, "size", "height");
        assert_eq!(width, scale.pixels(320.0));
        assert_eq!(height, scale.pixels(610.0));
        // Имитируем замену position движком: оба состояния должны сохранять исходные отступы.
        for (panel_x, original_right) in [(-225.0, 35.0), (-5.0, 255.0)] {
            let right = scale.pixels(panel_x) - 60.0 + dx + width;
            assert_eq!(right, scale.pixels(panel_x) + scale.pixels(-60.0) + width);
            assert!((right - scale.pixels(original_right)).abs() <= 1.0);
        }
        let top = number(panel, "position", "y") + 5.0 + dy;
        assert_eq!(top, scale.pixels(35.0) + scale.pixels(5.0));
        assert_eq!(
            compensation
                .field("background")
                .unwrap()
                .field("spriteType")
                .unwrap()
                .atom(),
            Some("GFX_navbar_bg_wide")
        );
    }
}

#[test]
fn trait_buttons_compensate_engine_positions() {
    let fixture = fixture();
    let root = fixture.0.join("game");
    write(
        &root,
        "interface/assign_trait_view.gui",
        r#"guiTypes = {
        containerWindowType = { name = "assign_trait_view"
            containerWindowType = { name = "bottom_container" position = { x = 0 y = 0 }
                buttonType = { name = "confirm_button" position = { x = 0 y = 11 } size = { x = 0 y = 0 } }
            }
            buttonType = { name = "close_button" position = { x = 0 y = -2 } }
        }
        containerWindowType = { name = "unrelated" containerWindowType = { name = "bottom_container" position = { x = 3 y = 4 } } }
    }"#,
    );
    let plan = scan_game(&root).unwrap();
    let output = fixture.0.join("mod");
    for factor in [0.5, 1.0, 1.3846, 2.0] {
        let scale = Scale::try_from(factor).unwrap();
        let profile = Profile {
            scale,
            ..Profile::default()
        };
        build_mod(&plan, &profile, &output).unwrap();
        let gui = fs::read_to_string(output.join("interface/assign_trait_view.gui"))
            .unwrap_or_else(|_| {
                fs::read_to_string(root.join("interface/assign_trait_view.gui")).unwrap()
            });
        let doc = Document::parse(&gui).unwrap();
        let panel = &doc.nodes[0].children()[0];
        let container = panel.field("containerWindowType").unwrap();
        let (offset_x, offset_y, bottom) = if container.named() == Some("bottom_container") {
            (0.0, 0.0, container)
        } else {
            assert_eq!(container.named(), Some("ui_scaler_trait_bottom_offset"));
            (
                number(container, "position", "x"),
                number(container, "position", "y"),
                container.field("containerWindowType").unwrap(),
            )
        };
        assert_eq!(bottom.named(), Some("bottom_container"));
        let button = bottom.field("buttonType").unwrap();
        assert_eq!(button.named(), Some("confirm_button"));
        let button_y = number(button, "position", "y");
        for count in [1, 2, 4] {
            let cards_height = f64::from(count) * scale.pixels(115.0);
            let actual = cards_height + 50.0 + offset_y + button_y;
            assert_eq!(
                actual,
                cards_height + scale.pixels(50.0) + scale.pixels(11.0)
            );
        }
        let close_wrapper = panel
            .children()
            .iter()
            .find(|node| node.named() == Some("ui_scaler_trait_close_offset"));
        let (close_x, close_y) = close_wrapper.map_or((0.0, 0.0), |wrapper| {
            let size = wrapper.field("size").unwrap();
            assert_eq!(size.field("width").unwrap().atom(), Some("100%"));
            assert_eq!(
                wrapper.field("buttonType").unwrap().named(),
                Some("close_button")
            );
            (
                number(wrapper, "position", "x"),
                number(wrapper, "position", "y"),
            )
        });
        for original_width in [354.0, 510.0, 680.0] {
            let window_width = scale.pixels(original_width);
            let native_button_width = window_width.max(450.0) - 300.0;
            let native_button_x = window_width - native_button_width - 15.0;
            assert_eq!(
                window_width - (offset_x + native_button_x + native_button_width),
                scale.pixels(15.0)
            );
            assert_eq!(
                window_width - (window_width - 41.0 + close_x),
                scale.pixels(41.0)
            );
            assert_eq!(9.0 + close_y, scale.pixels(9.0));
        }
        let unrelated = &doc.nodes[0].children()[1];
        assert_eq!(
            unrelated.field("containerWindowType").unwrap().named(),
            Some("bottom_container")
        );
    }
}
