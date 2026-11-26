use super::*;

#[test]
fn navbar_lock_keeps_native_offsets_after_engine_repositions_it() {
    let fixture = fixture();
    let root = fixture.0.join("game");
    let path = "interface/main.gui";
    write(
        &root,
        path,
        r#"guiTypes = {
            containerWindowType = { name = maingui_left_bar_buttons
                position = { x = -225 y = 35 } size = { width = 250 height = 690 }
                buttonType = { name = lock position = { x = -20 y = -112 }
                    Orientation = "LOWER_RIGHT" quadTextureSprite = GFX_test clicksound = interface
                }
                buttonType = { name = settings position = { x = -28 y = 20 } Orientation = "UPPER_RIGHT" }
                buttonType = { name = reset_ordering_button position = { x = -190 y = 2 } Orientation = "UPPER_RIGHT" }
                containerWindowType = { name = background size = { width = 320 height = 610 } position = { x = 5 y = 5 } }
                gridBoxType = { name = gridbox position = { x = 34 y = 50 } slotSize = { width = 54 height = 40 } }
            }
            containerWindowType = { name = unrelated
                buttonType = { name = lock position = { x = 10 y = 20 } }
            }
        }"#,
    );
    let plan = scan_game(&root).unwrap();
    let output = fixture.0.join("mod");
    for factor in [0.5, 1.0, 1.3846, 1.86921, 2.0] {
        let scale = Scale::try_from(factor).unwrap();
        let profile = Profile {
            scale,
            ..Profile::default()
        };
        build_mod(&plan, &profile, &output).unwrap();
        let gui = fs::read_to_string(output.join(path))
            .unwrap_or_else(|_| fs::read_to_string(root.join(path)).unwrap());
        let doc = Document::parse(&gui).unwrap();
        let panel = named(&doc.nodes[0], "maingui_left_bar_buttons");
        // Setup использует GetButton, а не GetButtonRecursive: lock обязан быть прямым потомком.
        let lock = named(panel, "lock");
        assert_eq!(lock.key, Some("buttonType"));
        assert_eq!(
            lock.field("orientation").unwrap().atom(),
            Some("LOWER_RIGHT")
        );
        assert_eq!(lock.field("clicksound").unwrap().atom(), Some("interface"));
        // Движок перезаписывает координаты кнопки; размер панели компенсирует отступы.
        for panel_x in [-225.0, -5.0] {
            let right = scale.pixels(panel_x) + number(panel, "size", "width");
            let original_right = scale.pixels(panel_x) + scale.pixels(250.0);
            assert_eq!(right - 20.0, original_right + scale.pixels(-20.0));
            for (name, x) in [("settings", -28.0), ("reset_ordering_button", -190.0)] {
                assert_eq!(
                    right + number(named(panel, name), "position", "x"),
                    original_right + scale.pixels(x)
                );
            }
        }
        let bottom = number(panel, "position", "y") + number(panel, "size", "height");
        assert_eq!(
            bottom - 112.0,
            scale.pixels(35.0) + scale.pixels(690.0) + scale.pixels(-112.0)
        );
        assert_eq!(
            number(named(panel, "background"), "size", "width"),
            scale.pixels(320.0)
        );
        assert_eq!(
            number(named(panel, "background"), "size", "height"),
            scale.pixels(610.0)
        );
        assert_eq!(
            number(named(panel, "gridbox"), "position", "x"),
            scale.pixels(34.0)
        );
        assert_eq!(
            number(named(panel, "gridbox"), "slotSize", "height"),
            scale.pixels(40.0)
        );
        let other = named(named(&doc.nodes[0], "unrelated"), "lock");
        assert_eq!(number(other, "position", "x"), scale.pixels(10.0));
    }
}
