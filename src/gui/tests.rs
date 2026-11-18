use std::{collections::BTreeMap, sync::Arc};

use eframe::egui::{self, Color32, Pos2, Rect, Vec2};
use egui::accesskit::{Action, ActionData, ActionRequest, NodeId, TreeId};

use crate::{
    assets::Installation,
    compiler::{BuildPlan, TextureAsset},
    config::{Category, Profile},
    fonts,
};

use super::{ComparisonMode, Selection, Studio, WorkspaceTab, icons, theme};

pub(super) fn action(action: Action, target_node: NodeId, data: Option<ActionData>) -> egui::Event {
    egui::Event::AccessKitActionRequest(ActionRequest {
        action,
        target_node,
        target_tree: TreeId::ROOT,
        data,
    })
}

pub(super) fn node(
    output: &egui::FullOutput,
    predicate: impl Fn(&egui::accesskit::Node) -> bool,
) -> NodeId {
    output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .find(|(_, node)| predicate(node))
        .map(|(id, _)| *id)
        .unwrap()
}

fn workspace(ctx: &egui::Context) -> Studio {
    let path =
        "gfx/interface/a_very_long_folder_name/nested_resources/a_very_long_texture_filename.dds";
    let mut studio = Studio::new(None);
    studio.profile = Profile::default();
    studio.plan = Some(Arc::new(BuildPlan {
        game: Installation {
            source_mods: Vec::new(),
            root: "unused".into(),
            version: "v4.5.2".into(),
            resources: BTreeMap::new(),
            archives: 0,
        },
        fonts: fonts::Catalog {
            groups: vec![fonts::Group {
                name: "standard_font".into(),
                ..Default::default()
            }],
            ..Default::default()
        },
        sources: BTreeMap::new(),
        sprites: Vec::new(),
        diagnostics: Vec::new(),
        textures: vec![TextureAsset {
            path: path.into(),
            width: 32,
            height: 32,
            frames: 1,
            category: Category::Icon,
            tiled: false,
        }],
    }));
    studio.selected = Some(0);
    studio.rule_selection = Some(Selection::Asset(path.into()));
    studio.filter = "very_long".into();
    studio.refresh_asset_filter();
    studio.asset_tree.expand_all();
    studio.images = [("Original", 32), ("1: Lanczos3", 64), ("2: CatmullRom", 64)]
        .into_iter()
        .map(|(name, side)| {
            (
                name.into(),
                super::PreviewImage::Ready(ctx.load_texture(
                    name,
                    egui::ColorImage::filled([side, side], Color32::WHITE),
                    egui::TextureOptions::NEAREST,
                )),
            )
        })
        .collect();
    studio
}

#[test]
fn loading_discovery_settings_invalidates_sources_without_populating_manual_paths() {
    let temporary = crate::test_support::Temp::new();
    let directory = &temporary.0;
    let ctx = egui::Context::default();
    let mut studio = workspace(&ctx);
    studio.profile.source_mods.push("manual-override".into());
    let mut profile = studio.profile.clone();
    profile.auto_source_mods = Some(crate::config::AutoSourceMods::default());
    let path = directory.join("profile.toml");
    profile.save(&path).unwrap();
    studio.profile_path = path.display().to_string();
    studio.load_profile();
    assert_eq!(
        studio.profile.source_mods,
        [std::path::PathBuf::from("manual-override")]
    );
    assert_eq!(studio.profile.auto_source_mods, profile.auto_source_mods);
    assert!(studio.plan.is_none());
    assert!(studio.selected.is_none());
    assert!(studio.images.is_empty());
}

fn finish_scan(studio: &mut Studio, ctx: &egui::Context) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while studio.receiver.is_some() && std::time::Instant::now() < deadline {
        studio.poll(ctx);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(studio.receiver.is_none(), "{}", studio.status);
}

#[test]
fn scan_loads_enabled_mods_preserves_choices_and_keeps_the_old_snapshot_on_failure() {
    use std::fs;
    let temporary = crate::test_support::Temp::new();
    let directory = &temporary.0;
    let game = directory.join("game");
    let data = directory.join("user");
    let source = data.join("mod/enabled");
    for root in [&game, &source] {
        fs::create_dir_all(root.join("interface")).unwrap();
        fs::create_dir_all(root.join("gfx")).unwrap();
        fs::write(
            root.join("interface/example.gui"),
            "iconType = { name = test position = { x = 11 y = 0 } }",
        )
        .unwrap();
    }
    fs::write(
        game.join("launcher-settings.json"),
        r#"{"rawVersion":"v4.5.2"}"#,
    )
    .unwrap();
    let ctx = egui::Context::default();
    let mut studio = workspace(&ctx);
    studio.game = game.display().to_string();
    studio.profile.auto_source_mods = Some(crate::config::AutoSourceMods {
        user_data_dir: Some(data.clone()),
        source_mods: vec!["previously-enabled".into()],
    });
    studio
        .profile
        .source_mods
        .push("ignored-manual-source".into());
    let previous = studio.profile.auto_source_mods.clone();
    studio.scan_assets(&ctx);
    finish_scan(&mut studio, &ctx);
    assert_eq!(studio.profile.auto_source_mods, previous);
    assert!(studio.plan.is_none());
    assert!(studio.status.contains("dlc_load.json"));
    fs::write(
        data.join("dlc_load.json"),
        r#"{"enabled_mods":["mod/enabled.mod"]}"#,
    )
    .unwrap();
    fs::write(data.join("mod/enabled.mod"), "path = mod/enabled").unwrap();
    studio.scan_assets(&ctx);
    finish_scan(&mut studio, &ctx);
    let source = source.canonicalize().unwrap();
    assert_eq!(
        studio
            .profile
            .auto_source_mods
            .as_ref()
            .unwrap()
            .source_mods
            .as_slice(),
        std::slice::from_ref(&source)
    );
    assert_eq!(studio.profile.source_mods, Vec::<std::path::PathBuf>::new());
    assert_eq!(
        studio.plan.as_ref().unwrap().game.source_mods,
        Vec::<std::path::PathBuf>::new()
    );
    assert!(studio.profile.disabled_source_mods.contains(&source));
    assert!(studio.status.contains("Enabled UI mods changed"));
    studio.profile.disabled_source_mods.remove(&source);
    studio.scan_assets(&ctx);
    finish_scan(&mut studio, &ctx);
    assert_eq!(
        studio.plan.as_ref().unwrap().game.source_mods.as_slice(),
        std::slice::from_ref(&source)
    );
    let profile_path = directory.join("profile.toml");
    studio.profile.save(&profile_path).unwrap();
    let saved = Profile::load(&profile_path).unwrap();
    assert_eq!(saved.auto_source_mods, studio.profile.auto_source_mods);
    assert_eq!(
        saved.disabled_source_mods,
        studio.profile.disabled_source_mods
    );
}

#[test]
fn workspace_panels_stay_inside_the_viewport_with_long_asset_names() {
    for size in [Vec2::new(1100.0, 700.0), Vec2::new(1600.0, 1000.0)] {
        let ctx = egui::Context::default();
        theme::install(&ctx);
        icons::install(&ctx).unwrap();
        let mut studio = workspace(&ctx);
        for tab in [
            WorkspaceTab::Textures,
            WorkspaceTab::Project,
            WorkspaceTab::Fonts,
            WorkspaceTab::ModDetails,
        ] {
            studio.tab = tab;
            for _ in 0..10 {
                let mut used = Rect::NOTHING;
                ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                        ..Default::default()
                    },
                    |ui| {
                        studio.show(ui);
                        used = ui.min_rect();
                        if tab == WorkspaceTab::Textures {
                            let browser = egui::containers::panel::PanelState::load(
                                &ctx,
                                ui.scope_id().with("asset-browser"),
                            )
                            .unwrap();
                            assert!(
                                browser.size().x <= 271.0,
                                "Asset browser grows without resizing: {:?}",
                                browser.size()
                            );
                            let inspector = egui::containers::panel::PanelState::load(
                                &ctx,
                                ui.scope_id().with("asset-inspector"),
                            )
                            .unwrap();
                            assert!(
                                inspector.size().x <= 281.0,
                                "Inspector grows without resizing: {:?}",
                                inspector.size()
                            );
                        }
                    },
                )
                .drop_without_applying_deltas();
                assert!(
                    used.right() <= size.x + 1.0 && used.bottom() <= size.y + 1.0,
                    "UI exceeds viewport: {used:?}, {size:?}"
                );
            }
        }
    }
}

#[test]
fn clicking_a_preview_image_or_title_applies_the_algorithm_to_the_current_selection() {
    use crate::config::TextureRule;

    for selection in [
        Selection::Asset("gfx/icons/test.dds".into()),
        Selection::Directory("gfx/icons".into()),
        Selection::Category(Category::Icon),
    ] {
        for click_image in [true, false] {
            let ctx = egui::Context::default();
            ctx.enable_accesskit();
            let mut studio = workspace(&ctx);
            studio.rule_selection = Some(selection.clone());
            let index = usize::from(!click_image);
            let algorithm = studio.algorithms[index];
            let textures: Vec<_> = studio
                .images
                .iter()
                .map(|(_, image)| image.handle().unwrap().id())
                .collect();
            let texture = textures[index + 1];
            let mut point = None;
            let mut title = None;
            for _ in 0..3 {
                let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                    studio.previews_panel(ui);
                });
                point = output.shapes.iter().find_map(|shape| {
                    if let egui::Shape::Mesh(mesh) = &shape.shape
                        && mesh.texture_id == texture
                    {
                        Some(mesh.calc_bounds().center())
                    } else {
                        None
                    }
                });
                title = Some(node(&output, |node| node.label() == Some("2: CatmullRom")));
                output.drop_without_applying_deltas();
            }
            let events = if click_image {
                let point = point.unwrap();
                vec![
                    vec![egui::Event::PointerMoved(point)],
                    vec![egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    }],
                    vec![egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: egui::Modifiers::NONE,
                    }],
                ]
            } else {
                vec![vec![action(Action::Click, title.unwrap(), None)]]
            };
            for events in events {
                ctx.run_ui(
                    egui::RawInput {
                        events,
                        ..Default::default()
                    },
                    |ui| studio.previews_panel(ui),
                )
                .drop_without_applying_deltas();
            }
            assert_eq!(studio.chosen, index);
            let rules = match &selection {
                Selection::Asset(_) => &studio.profile.assets,
                Selection::Directory(_) => &studio.profile.directories,
                Selection::Category(_) => &studio.profile.categories,
            };
            assert_eq!(
                rules[selection.key()],
                TextureRule {
                    algorithm: Some(algorithm),
                    ..Default::default()
                }
            );
            assert!(!studio.preview_dirty);
            assert!(studio.receiver.is_none());
            assert_eq!(
                studio
                    .images
                    .iter()
                    .map(|(_, image)| image.handle().unwrap().id())
                    .collect::<Vec<_>>(),
                textures
            );
        }
    }
}

#[test]
fn comparison_highlight_tracks_the_current_algorithm_and_clears_for_missing_candidates() {
    use crate::config::Algorithm;

    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut studio = workspace(&ctx);
    studio.count = 2;
    let textures: Vec<_> = studio
        .images
        .iter()
        .map(|(_, image)| image.handle().unwrap().id())
        .collect();
    for (algorithm, selected) in [
        (Algorithm::Bilinear, None),
        (Algorithm::CatmullRom, Some(1)),
        (Algorithm::Nearest, None),
        (Algorithm::Lanczos3, Some(0)),
    ] {
        studio
            .rule_selection
            .as_ref()
            .unwrap()
            .set_algorithm(&mut studio.profile, algorithm);
        assert_eq!(studio.current_algorithm_candidate(), selected);
        for _ in 0..2 {
            let output = ctx.run_ui(egui::RawInput::default(), |ui| studio.previews_panel(ui));
            for (index, label) in ["1: Lanczos3", "2: CatmullRom"].into_iter().enumerate() {
                let node = output
                    .platform_output
                    .accesskit_update
                    .as_ref()
                    .unwrap()
                    .nodes
                    .iter()
                    .find(|(_, node)| node.label() == Some(label))
                    .unwrap();
                assert_eq!(
                    node.1.toggled(),
                    Some(if selected == Some(index) {
                        egui::accesskit::Toggled::True
                    } else {
                        egui::accesskit::Toggled::False
                    })
                );
            }
            let borders = output.shapes.iter().filter(|shape| {
                matches!(&shape.shape, egui::Shape::Rect(rect) if rect.stroke.color == theme::MINT)
            }).count();
            assert_eq!(borders, usize::from(selected.is_some()));
            output.drop_without_applying_deltas();
        }
        assert!(!studio.preview_dirty);
        assert!(studio.receiver.is_none());
        assert_eq!(
            studio
                .images
                .iter()
                .map(|(_, image)| image.handle().unwrap().id())
                .collect::<Vec<_>>(),
            textures
        );
    }
    studio.selected = None;
    assert_eq!(studio.current_algorithm_candidate(), None);
}

#[test]
fn candidate_navigation_applies_the_algorithm_without_rebuilding_previews() {
    for mode in [ComparisonMode::Split, ComparisonMode::Ab] {
        let ctx = egui::Context::default();
        let mut studio = workspace(&ctx);
        studio.mode = mode;
        let path = studio.rule_selection.as_ref().unwrap().key().to_owned();
        let textures: Vec<_> = studio
            .images
            .iter()
            .map(|(_, image)| image.handle().unwrap().id())
            .collect();
        for (key, index) in [(egui::Key::ArrowRight, 1), (egui::Key::ArrowLeft, 0)] {
            ctx.run_ui(
                egui::RawInput {
                    events: vec![egui::Event::Key {
                        key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    }],
                    ..Default::default()
                },
                |ui| studio.previews_panel(ui),
            )
            .drop_without_applying_deltas();
            assert_eq!(studio.chosen, index);
            assert_eq!(
                studio.profile.assets[&path].algorithm,
                Some(studio.algorithms[index])
            );
            assert!(!studio.preview_dirty);
            assert!(studio.receiver.is_none());
            assert_eq!(
                studio
                    .images
                    .iter()
                    .map(|(_, image)| image.handle().unwrap().id())
                    .collect::<Vec<_>>(),
                textures
            );
        }
    }
}

#[test]
fn middle_button_pans_previews_without_changing_the_selection_or_split() {
    for mode in [
        ComparisonMode::Compare,
        ComparisonMode::Ab,
        ComparisonMode::Split,
    ] {
        let ctx = egui::Context::default();
        let mut studio = workspace(&ctx);
        studio.mode = mode;
        studio.fit_preview = false;
        studio.zoom = 32.0;
        let before = toml::to_string(&studio.profile).unwrap();
        let split = studio.split;
        let start = Pos2::new(150.0, 150.0);
        let end = start - Vec2::new(40.0, 30.0);
        for events in [
            vec![],
            vec![
                egui::Event::PointerMoved(start),
                egui::Event::PointerButton {
                    pos: start,
                    button: egui::PointerButton::Middle,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            vec![egui::Event::PointerMoved(end)],
            vec![egui::Event::PointerButton {
                pos: end,
                button: egui::PointerButton::Middle,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
            vec![],
        ] {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(640.0, 480.0))),
                    events,
                    ..Default::default()
                },
                |ui| studio.previews_panel(ui),
            )
            .drop_without_applying_deltas();
        }
        assert_eq!(studio.preview_offset, Vec2::new(40.0, 30.0));
        assert_eq!(studio.chosen, 0);
        assert_eq!(studio.split, split);
        assert_eq!(toml::to_string(&studio.profile).unwrap(), before);
    }
}

#[test]
fn split_drag_changes_the_comparison_without_changing_the_profile() {
    let ctx = egui::Context::default();
    let mut studio = workspace(&ctx);
    studio.fit_preview = true;
    let before = toml::to_string(&studio.profile).unwrap();
    let canvas = Vec2::splat(400.0);
    let point = Pos2::new(290.0, 200.0);
    for events in [
        vec![],
        vec![
            egui::Event::PointerMoved(point),
            egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ],
        vec![egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
    ] {
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, canvas)),
                events,
                ..Default::default()
            },
            |ui| {
                if let Some(split) = studio
                    .draw_image(
                        ui,
                        studio.images[0].1.handle().unwrap(),
                        studio.images[1].1.handle(),
                        canvas,
                    )
                    .1
                {
                    studio.split = split;
                }
            },
        )
        .drop_without_applying_deltas();
    }
    assert!((studio.split - 0.75).abs() < 0.01);
    assert_eq!(toml::to_string(&studio.profile).unwrap(), before);
    for mode in [
        ComparisonMode::Compare,
        ComparisonMode::Ab,
        ComparisonMode::Split,
    ] {
        studio.mode = mode;
        ctx.run_ui(egui::RawInput::default(), |ui| studio.previews_panel(ui))
            .drop_without_applying_deltas();
    }
}

#[test]
fn editable_game_combo_centers_text_and_matches_menu_width() {
    let ctx = egui::Context::default();
    theme::install(&ctx);
    let mut studio = Studio::new(Some("/games/Stellaris".into()));
    studio.game_locations.clear();
    let mut combo_rect = Rect::NOTHING;
    let mut combo_id = egui::Id::NULL;
    let mut text_center = None;
    for _ in 0..3 {
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let response = studio.game_path_field(ui, 340.0);
            combo_rect = response.rect;
            combo_id = response.id;
        });
        for shape in &output.shapes {
            if let egui::Shape::Text(text) = &shape.shape
                && text.galley.text() == studio.game
            {
                text_center = Some(text.pos.y + text.galley.rect.center().y);
            }
        }
        output.drop_without_applying_deltas();
    }
    assert!((combo_rect.width() - 340.0).abs() < 1.0, "{combo_rect:?}");
    assert!(
        (text_center.unwrap() - combo_rect.center().y).abs() < 1.0,
        "text center: {text_center:?}, combo: {combo_rect:?}"
    );

    let point = Pos2::new(combo_rect.right() - 15.0, combo_rect.center().y);
    for events in [
        vec![egui::Event::PointerMoved(point)],
        vec![egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }],
        vec![egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
        vec![],
        vec![],
    ] {
        ctx.run_ui(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ui| {
                studio.game_path_field(ui, 340.0);
            },
        )
        .drop_without_applying_deltas();
    }
    assert!(egui::ComboBox::is_open(&ctx, combo_id));
    let menu = egui::AreaState::load(&ctx, combo_id.with("popup"))
        .unwrap()
        .rect();
    assert!(
        (menu.width() - combo_rect.width()).abs() < 1.0,
        "{menu:?}, {combo_rect:?}"
    );
}

#[test]
fn editing_the_game_path_discards_the_previous_scan_and_build() {
    let ctx = egui::Context::default();
    let mut studio = workspace(&ctx);
    studio.game = "/games/Stellaris".into();
    studio.built_mod = Some("dist/previous-build".into());
    let mut input_id = egui::Id::NULL;
    for _ in 0..3 {
        ctx.run_ui(egui::RawInput::default(), |ui| {
            input_id = studio.game_path_field(ui, 340.0).id.with("input");
        })
        .drop_without_applying_deltas();
    }
    assert!(studio.plan.is_some());
    assert!(studio.built_mod.is_some());
    ctx.memory_mut(|memory| memory.request_focus(input_id));
    ctx.run_ui(
        egui::RawInput {
            events: vec![egui::Event::Text("-other".into())],
            ..Default::default()
        },
        |ui| {
            assert!(studio.game_path_field(ui, 340.0).changed());
        },
    )
    .drop_without_applying_deltas();
    assert_eq!(studio.game, "/games/Stellaris-other");
    assert!(studio.plan.is_none());
    assert!(studio.built_mod.is_none());
    assert!(studio.selected.is_none());
    assert!(studio.rule_selection.is_none());
    assert!(studio.images.is_empty());
    assert_eq!(studio.asset_search.matches, []);
    assert!(studio.status.contains("scan assets again"));
}

#[test]
fn search_shortcuts_switch_tabs_and_clear_only_the_focused_search() {
    let ctx = egui::Context::default();
    let mut studio = workspace(&ctx);
    studio.tab = WorkspaceTab::Project;
    let press = |key, modifiers| egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1600.0, 1000.0))),
        events: vec![egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }],
        ..Default::default()
    };
    ctx.run_ui(press(egui::Key::Escape, egui::Modifiers::NONE), |ui| {
        studio.show(ui);
    })
    .drop_without_applying_deltas();
    assert_eq!(studio.filter, "very_long");
    ctx.run_ui(press(egui::Key::F, egui::Modifiers::COMMAND), |ui| {
        studio.show(ui);
    })
    .drop_without_applying_deltas();
    let search_id = egui::Id::unique("asset-search");
    assert!(studio.tab == WorkspaceTab::Textures);
    assert!(ctx.memory(|memory| memory.has_focus(search_id)));
    assert_eq!(studio.filter, "very_long");
    ctx.run_ui(press(egui::Key::Escape, egui::Modifiers::NONE), |ui| {
        studio.show(ui);
    })
    .drop_without_applying_deltas();
    assert_eq!(studio.filter, "");
    assert_eq!(studio.asset_search.matches, [(0, 0)]);
    assert!(ctx.memory(|memory| memory.has_focus(search_id)));
}

#[test]
fn preview_results_are_visible_before_completion_and_keep_candidate_indices() {
    use super::{Event, PreviewImage};
    use crate::compiler::PreviewEvent;
    use crate::config::Algorithm;
    let ctx = egui::Context::default();
    let mut studio = workspace(&ctx);
    studio.count = 2;
    let (sender, receiver) = std::sync::mpsc::channel();
    studio.receiver = Some(receiver);
    sender
        .send(Event::Preview(PreviewEvent::Source {
            image: Arc::new(image::RgbaImage::new(32, 32)),
            features: None,
        }))
        .unwrap();
    sender
        .send(Event::Preview(PreviewEvent::Candidate {
            index: 1,
            image: Ok(image::RgbaImage::new(64, 64)),
        }))
        .unwrap();
    studio.poll(&ctx);
    assert!(studio.receiver.is_some());
    assert!(matches!(studio.images[1].1, PreviewImage::Pending));
    assert!(matches!(studio.images[2].1, PreviewImage::Ready(_)));
    assert_eq!(studio.images[2].0, "2: CatmullRom");
    assert_eq!(studio.images[2].1.handle().unwrap().size(), [64, 64]);
    let ready = studio.images[2].1.handle().unwrap().id();
    for mode in [
        ComparisonMode::Compare,
        ComparisonMode::Ab,
        ComparisonMode::Split,
    ] {
        studio.mode = mode;
        ctx.run_ui(egui::RawInput::default(), |ui| studio.previews_panel(ui))
            .drop_without_applying_deltas();
    }
    sender
        .send(Event::Preview(PreviewEvent::Candidate {
            index: 0,
            image: Err(anyhow::anyhow!("Inference failed")),
        }))
        .unwrap();
    studio.poll(&ctx);
    assert!(
        matches!(&studio.images[1].1, PreviewImage::Failed(error) if error == "Inference failed")
    );
    assert_eq!(studio.images[2].1.handle().unwrap().id(), ready);
    studio.chosen = 0;
    let before = toml::to_string(&studio.profile).unwrap();
    studio.apply_selected_preview();
    assert_eq!(toml::to_string(&studio.profile).unwrap(), before);
    studio.chosen = 1;
    studio.apply_selected_preview();
    assert_eq!(
        studio
            .selected_asset()
            .unwrap()
            .options(&studio.profile)
            .algorithm,
        Algorithm::CatmullRom
    );
    sender
        .send(Event::Finished(Ok(super::Completed::Preview)))
        .unwrap();
    studio.poll(&ctx);
    assert!(studio.receiver.is_none());
    assert_eq!(studio.images[2].1.handle().unwrap().id(), ready);
}
