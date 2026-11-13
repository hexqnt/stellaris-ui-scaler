use egui::accesskit::Action;

use crate::{
    config::AutoSourceMods,
    gui::tests::{action, node},
};

use super::*;

fn render(
    ctx: &egui::Context,
    editor: &mut Editor,
    profile: &mut Profile,
    events: Vec<egui::Event>,
) -> (egui::FullOutput, bool) {
    let mut response = false;
    let output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(780.0, 650.0),
            )),
            events,
            ..Default::default()
        },
        |ui| response = editor.show(ui, profile),
    );
    (output, response)
}

#[test]
fn checkboxes_select_sources_without_changing_the_saved_launcher_list() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let first = PathBuf::from("unused/first");
    let second = PathBuf::from("unused/second");
    let mut profile = Profile {
        auto_source_mods: Some(AutoSourceMods {
            source_mods: vec![first.clone(), second.clone()],
            ..Default::default()
        }),
        ..Profile::default()
    };
    let mut editor = Editor::default();
    editor.names.insert(first.clone(), "First UI mod".into());
    editor.names.insert(second.clone(), "Second UI mod".into());
    let (output, _) = render(&ctx, &mut editor, &mut profile, Vec::new());
    let nodes = &output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes;
    assert_eq!(
        nodes
            .iter()
            .filter(|(_, node)| node.role() == egui::Role::CheckBox)
            .count(),
        2
    );
    assert!(nodes.iter().all(|(_, node)| !matches!(
        node.label(),
        Some("Find enabled mods" | "Add folder" | "Select all" | "Select none")
    )));
    let checkbox = node(&output, |node| node.role() == egui::Role::CheckBox);
    output.drop_without_applying_deltas();
    let (output, response) = render(
        &ctx,
        &mut editor,
        &mut profile,
        vec![action(Action::Click, checkbox, None)],
    );
    output.drop_without_applying_deltas();
    assert!(response);
    assert!(profile.disabled_source_mods.contains(&first));
    assert_eq!(
        profile.auto_source_mods.as_ref().unwrap().source_mods,
        [first, second]
    );
    let (output, response) = render(
        &ctx,
        &mut editor,
        &mut profile,
        vec![action(Action::Click, checkbox, None)],
    );
    output.drop_without_applying_deltas();
    assert!(response);
    assert_eq!(
        profile.disabled_source_mods,
        std::collections::BTreeSet::new()
    );
}

#[test]
fn table_with_long_names_and_paths_stays_within_the_viewport() {
    let ctx = egui::Context::default();
    let mut editor = Editor::default();
    let mut profile = Profile {
        auto_source_mods: Some(AutoSourceMods::default()),
        ..Profile::default()
    };
    for index in 0..30 {
        let path = PathBuf::from(format!(
            "unused/very_long_mod_directory_{index}/{}",
            "nested/".repeat(30)
        ));
        editor
            .names
            .insert(path.clone(), "A very long readable mod name ".repeat(12));
        profile
            .auto_source_mods
            .as_mut()
            .unwrap()
            .source_mods
            .push(path);
    }
    for _ in 0..3 {
        let mut used = egui::Rect::NOTHING;
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(780.0, 650.0),
                )),
                ..Default::default()
            },
            |ui| {
                editor.show(ui, &mut profile);
                used = ui.min_rect();
            },
        )
        .drop_without_applying_deltas();
        assert!(used.right() <= 781.0 && used.bottom() <= 651.0, "{used:?}");
    }
}
