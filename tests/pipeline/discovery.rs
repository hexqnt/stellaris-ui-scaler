use stellaris_ui_scaler::config::AutoSourceMods;
use stellaris_ui_scaler::source_mods;

use super::*;

pub fn export(user_data: &Path, enabled: &[&str]) {
    write(
        user_data,
        "dlc_load.json",
        &serde_json::json!({ "enabled_mods": enabled, "disabled_dlcs": [] }).to_string(),
    );
}

pub fn descriptor(user_data: &Path, name: &str, path: &Path, extra: &str) {
    let path = path
        .to_str()
        .unwrap()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    write(
        user_data,
        name,
        &format!("name = test\npath = \"{path}\"\n{extra}"),
    );
}

#[test]
fn launch_export_preserves_priority_and_excludes_generated_and_non_ui_mods() {
    let fixture = fixture();
    let user_data = fixture.0.join("user");
    let first = user_data.join("mod/first");
    let last = fixture.0.join("external mod");
    for (root, x) in [(&first, 11), (&last, 23)] {
        write(
            root,
            "interface/example.gui",
            &format!("iconType = {{ name = test position = {{ x = {x} y = 0 }} }}"),
        );
    }
    let generated = user_data.join("mod/generated");
    write(&generated, compiler::manifest_name(), "{}");
    write(
        &generated,
        "interface/example.gui",
        "invalid generated source",
    );
    write(&user_data, "mod/voice/sound/test.asset", "voice");
    descriptor(&user_data, "mod/first.mod", Path::new("mod/first"), "");
    descriptor(&user_data, "mod/last.mod", &last, "");
    descriptor(
        &user_data,
        "mod/generated.mod",
        Path::new("mod/generated"),
        "",
    );
    descriptor(
        &user_data,
        "mod/voice.mod",
        Path::new("mod/voice"),
        "replace_path = common/species",
    );
    export(
        &user_data,
        &[
            "mod/first.mod",
            "mod/last.mod",
            "mod\\first.mod",
            "mod/generated.mod",
            "mod/voice.mod",
        ],
    );
    let before = fs::read(user_data.join("dlc_load.json")).unwrap();
    let discovered = source_mods::discover(Path::new("unused"), Some(&user_data)).unwrap();
    assert_eq!(discovered.user_data, user_data.canonicalize().unwrap());
    assert_eq!(
        discovered.source_mods,
        [last.canonicalize().unwrap(), first.canonicalize().unwrap()]
    );
    assert_eq!(discovered.skipped_mods, 2);
    let plan = compiler::plan_with_mods(
        &fixture.0.join("game"),
        &discovered.source_mods,
        &Control::default(),
        &mut |_| {},
    )
    .unwrap();
    assert!(plan.sources["interface/example.gui"].contains("x = 11"));
    assert_eq!(fs::read(user_data.join("dlc_load.json")).unwrap(), before);
}

#[test]
fn discovery_resolves_user_data_from_both_game_directory_forms() {
    let fixture = fixture();
    let game = fixture.0.join("game");
    let user_data = fixture.0.join("user");
    export(&user_data, &[]);
    write(
        &game,
        "launcher-settings.json",
        &serde_json::json!({
            "rawVersion": "v4.5.2", "gameDataPath": user_data
        })
        .to_string(),
    );
    for root in [&game, &fixture.0] {
        let discovered = source_mods::discover(root, None).unwrap();
        assert_eq!(discovered.source_mods, Vec::<PathBuf>::new());
        assert_eq!(discovered.user_data, user_data.canonicalize().unwrap());
    }
}

#[test]
fn unsupported_enabled_mods_fail_without_returning_a_partial_list() {
    let fixture = fixture();
    let user_data = fixture.0.join("user");
    write(
        &user_data,
        "mod/ui/interface/example.gui",
        "iconType = { name = test }",
    );
    descriptor(&user_data, "mod/ui.mod", Path::new("mod/ui"), "");
    export(&user_data, &["mod/ui.mod", "mod/unsupported.mod"]);
    for (source, expected) in [
        ("archive = mod/example.zip", "Archived source mods"),
        (
            "path = mod/ui\nreplace_path = interface",
            "replace_path affecting UI",
        ),
        (
            "path = mod/ui\nreplace_path = GFX/interface/",
            "replace_path affecting UI",
        ),
        (
            "path = mod/ui\nreplace_path = fonts",
            "replace_path affecting UI",
        ),
        (
            "path = mod/ui\nreplace_path = localisation",
            "replace_path affecting UI",
        ),
        (
            "path = mod/ui\nreplace_path = unchecked_defines",
            "replace_path affecting UI",
        ),
        ("name = broken", "has no path"),
        ("path = \"\"", "empty path"),
        ("path = mod/ui\npath = mod/other", "multiple paths"),
        ("path = mod/missing", "Cannot locate enabled mod"),
        ("path = {", "Unclosed block"),
    ] {
        write(&user_data, "mod/unsupported.mod", source);
        let error = source_mods::discover(Path::new("unused"), Some(&user_data)).unwrap_err();
        let error = format!("{error:#}");
        assert!(
            error.contains("unsupported.mod") && error.contains(expected),
            "{source}: {error}"
        );
    }
    descriptor(&user_data, "mod/unsupported.mod", Path::new("mod/ui"), "");
    write(
        &user_data,
        "mod/ui/descriptor.mod",
        "replace_path = interface",
    );
    let error = source_mods::discover(Path::new("unused"), Some(&user_data)).unwrap_err();
    assert!(format!("{error:#}").contains("replace_path affecting UI"));
}

#[test]
fn missing_or_invalid_exports_and_unsafe_descriptor_paths_are_errors() {
    let fixture = fixture();
    let user_data = &fixture.0;
    assert!(source_mods::discover(Path::new("unused"), Some(user_data)).is_err());
    for source in [
        "{",
        "{}",
        r#"{"enabled_mods":null}"#,
        r#"{"enabled_mods":[17]}"#,
    ] {
        write(user_data, "dlc_load.json", source);
        assert!(source_mods::discover(Path::new("unused"), Some(user_data)).is_err());
    }
    for path in [
        "../escape.mod",
        "/absolute.mod",
        "C:/absolute.mod",
        "mod/missing.mod",
    ] {
        export(user_data, &[path]);
        assert!(source_mods::discover(Path::new("unused"), Some(user_data)).is_err());
    }
}

#[test]
fn saved_mod_snapshot_is_checked_before_scanning_and_building() {
    let fixture = fixture();
    let game = fixture.0.join("game");
    let user_data = fixture.0.join("user");
    let first = user_data.join("mod/first");
    let second = user_data.join("mod/second");
    for (root, x) in [(&first, 11), (&second, 23)] {
        write(
            root,
            "interface/example.gui",
            &format!("iconType = {{ name = test position = {{ x = {x} y = 0 }} }}"),
        );
    }
    descriptor(&user_data, "mod/first.mod", &first, "");
    descriptor(&user_data, "mod/second.mod", &second, "");
    export(&user_data, &["mod/first.mod"]);
    let profile = Profile {
        auto_source_mods: Some(AutoSourceMods {
            user_data_dir: Some(user_data.clone()),
            source_mods: vec![first.canonicalize().unwrap()],
        }),
        ..Profile::default()
    };
    let profile_path = fixture.0.join("profile.toml");
    profile.save(&profile_path).unwrap();
    let saved = fs::read(&profile_path).unwrap();
    let mut loaded = Profile::load(&profile_path).unwrap();
    assert_eq!(loaded.auto_source_mods, profile.auto_source_mods);
    assert_eq!(loaded.source_mods, Vec::<PathBuf>::new());
    let original = scan_game_with_profile(&game, &loaded).unwrap();
    assert_eq!(original.game.source_mods, [first.canonicalize().unwrap()]);
    let output = fixture.0.join("compiled");
    build_mod(&original, &loaded, &output).unwrap();
    let before = fs::read(output.join(compiler::manifest_name())).unwrap();
    let manifest: compiler::Manifest = serde_json::from_slice(&before).unwrap();
    assert_eq!(manifest.source_mods, original.game.source_mods);
    assert_eq!(manifest.profile.source_mods, Vec::<PathBuf>::new());
    export(&user_data, &["mod/second.mod"]);
    let error = scan_game_with_profile(&game, &loaded).unwrap_err();
    let mismatch = error
        .downcast_ref::<source_mods::ModListMismatch>()
        .unwrap();
    assert_eq!(mismatch.saved, [first.canonicalize().unwrap()]);
    assert_eq!(mismatch.current, [second.canonicalize().unwrap()]);
    let error = build_mod(&original, &loaded, &output).unwrap_err();
    assert!(
        error
            .downcast_ref::<source_mods::ModListMismatch>()
            .is_some()
    );
    assert_eq!(
        fs::read(output.join(compiler::manifest_name())).unwrap(),
        before
    );
    assert_eq!(fs::read(&profile_path).unwrap(), saved);
    assert_eq!(
        loaded.auto_source_mods.as_ref().unwrap().source_mods,
        [first.canonicalize().unwrap()]
    );
    source_mods::refresh(&game, loaded.auto_source_mods.as_mut().unwrap()).unwrap();
    let refreshed = scan_game_with_profile(&game, &loaded).unwrap();
    assert_eq!(refreshed.game.source_mods, [second.canonicalize().unwrap()]);
    assert!(refreshed.sources["interface/example.gui"].contains("x = 23"));
    build_mod(&refreshed, &loaded, &output).unwrap();
    loaded.save(&profile_path).unwrap();
    assert_eq!(
        Profile::load(&profile_path)
            .unwrap()
            .auto_source_mods
            .unwrap()
            .source_mods,
        [second.canonicalize().unwrap()]
    );
    assert_eq!(loaded.source_mods, Vec::<PathBuf>::new());
}

#[test]
fn automatic_settings_roundtrip_and_disabling_discovery_ignores_the_export() {
    let fixture = fixture();
    let game = fixture.0.join("game");
    let user_data = fixture.0.join("user");
    let mut profile = Profile {
        auto_source_mods: Some(AutoSourceMods::default()),
        ..Profile::default()
    };
    let serialized = toml::to_string(&profile).unwrap();
    let loaded: Profile = toml::from_str(&serialized).unwrap();
    assert_eq!(loaded.auto_source_mods, profile.auto_source_mods);
    profile.auto_source_mods.as_mut().unwrap().user_data_dir = Some(user_data.clone());
    write(&user_data, "dlc_load.json", "broken export");
    assert!(scan_game_with_profile(&game, &profile).is_err());
    profile.auto_source_mods = None;
    let plan = scan_game_with_profile(&game, &profile).unwrap();
    assert_eq!(plan.game.source_mods, Vec::<PathBuf>::new());
    assert_eq!(
        plan.sources["interface/example.gui"],
        fs::read_to_string(game.join("interface/example.gui")).unwrap()
    );
    profile.auto_source_mods = Some(AutoSourceMods {
        user_data_dir: Some(PathBuf::new()),
        ..AutoSourceMods::default()
    });
    assert!(profile.check().is_err());
}

#[test]
fn order_changes_and_removed_mods_report_the_saved_and_current_lists() {
    let fixture = fixture();
    let game = fixture.0.join("game");
    let user_data = fixture.0.join("user");
    let first = user_data.join("mod/first");
    let second = user_data.join("mod/second");
    for root in [&first, &second] {
        write(root, "interface/example.gui", "iconType = { name = test }");
    }
    descriptor(&user_data, "mod/first.mod", &first, "");
    descriptor(&user_data, "mod/second.mod", &second, "");
    export(&user_data, &["mod/first.mod", "mod/second.mod"]);
    let mut profile = Profile {
        auto_source_mods: Some(AutoSourceMods {
            user_data_dir: Some(user_data.clone()),
            ..AutoSourceMods::default()
        }),
        ..Profile::default()
    };
    source_mods::refresh(&game, profile.auto_source_mods.as_mut().unwrap()).unwrap();
    export(&user_data, &["mod/second.mod", "mod/first.mod"]);
    let error = source_mods::resolve(&game, &profile).unwrap_err();
    let message = error.to_string();
    assert!(message.contains("Saved order:") && message.contains("Current order:"));
    let mismatch = error
        .downcast_ref::<source_mods::ModListMismatch>()
        .unwrap();
    assert_eq!(
        mismatch.saved,
        [
            first.canonicalize().unwrap(),
            second.canonicalize().unwrap()
        ]
    );
    assert_eq!(
        mismatch.current,
        [
            second.canonicalize().unwrap(),
            first.canonicalize().unwrap()
        ]
    );
    export(&user_data, &["mod/second.mod"]);
    fs::remove_dir_all(&first).unwrap();
    let error = source_mods::resolve(&game, &profile).unwrap_err();
    assert!(
        error
            .downcast_ref::<source_mods::ModListMismatch>()
            .is_some()
    );
    let previous = profile.auto_source_mods.clone();
    write(&user_data, "dlc_load.json", "broken export");
    assert!(source_mods::refresh(&game, profile.auto_source_mods.as_mut().unwrap()).is_err());
    assert_eq!(profile.auto_source_mods, previous);
}

#[test]
fn checkbox_choices_persist_and_filter_sources_before_overlaying_files() {
    let fixture = fixture();
    let game = fixture.0.join("game");
    let user_data = fixture.0.join("user");
    let first = user_data.join("mod/first");
    let last = user_data.join("mod/last");
    for (root, x, unique) in [(&first, 11, "first"), (&last, 23, "last")] {
        write(
            root,
            "interface/example.gui",
            &format!("iconType = {{ name = test position = {{ x = {x} y = 0 }} }}"),
        );
        write(
            root,
            &format!("interface/{unique}_only.gui"),
            "iconType = { name = unique }",
        );
    }
    descriptor(&user_data, "mod/first.mod", &first, "");
    descriptor(&user_data, "mod/last.mod", &last, "");
    export(&user_data, &["mod/first.mod", "mod/last.mod"]);
    let first = first.canonicalize().unwrap();
    let last = last.canonicalize().unwrap();
    let mut profile = Profile {
        auto_source_mods: Some(AutoSourceMods {
            user_data_dir: Some(user_data),
            source_mods: vec![first.clone(), last.clone()],
        }),
        ..Profile::default()
    };
    let both = scan_game_with_profile(&game, &profile).unwrap();
    assert!(both.sources["interface/example.gui"].contains("x = 23"));
    assert!(both.sources.contains_key("interface/first_only.gui"));
    assert!(both.sources.contains_key("interface/last_only.gui"));
    profile.disabled_source_mods.insert(last.clone());
    let path = fixture.0.join("profile.toml");
    profile.save(&path).unwrap();
    let mut restored = Profile::load(&path).unwrap();
    assert_eq!(restored.disabled_source_mods, profile.disabled_source_mods);
    assert_eq!(restored.auto_source_mods, profile.auto_source_mods);
    let selected = scan_game_with_profile(&game, &restored).unwrap();
    assert_eq!(
        selected.game.source_mods.as_slice(),
        std::slice::from_ref(&first)
    );
    assert!(selected.sources["interface/example.gui"].contains("x = 11"));
    assert!(selected.sources.contains_key("interface/first_only.gui"));
    assert!(!selected.sources.contains_key("interface/last_only.gui"));
    let error = build_mod(&both, &restored, &fixture.0.join("rejected")).unwrap_err();
    assert!(error.to_string().contains("scan assets again"));
    restored.disabled_source_mods.insert(first);
    let empty = scan_game_with_profile(&game, &restored).unwrap();
    assert_eq!(empty.game.source_mods, Vec::<PathBuf>::new());
    assert_eq!(
        empty.sources["interface/example.gui"],
        fs::read_to_string(game.join("interface/example.gui")).unwrap()
    );
    export(
        restored
            .auto_source_mods
            .as_ref()
            .unwrap()
            .user_data_dir
            .as_ref()
            .unwrap(),
        &["mod/first.mod"],
    );
    let error = source_mods::resolve(&game, &restored).unwrap_err();
    assert!(
        error
            .downcast_ref::<source_mods::ModListMismatch>()
            .is_some()
    );
    restored.auto_source_mods = None;
    restored.source_mods = vec![last];
    assert_eq!(
        source_mods::resolve(&game, &restored).unwrap(),
        Vec::<PathBuf>::new()
    );
}

#[test]
fn mod_table_names_use_metadata_with_a_folder_name_fallback() {
    let fixture = fixture();
    let root = fixture.0.join("Folder Name");
    assert_eq!(source_mods::mod_name(&root), "Folder Name");
    write(&root, "descriptor.mod", r#"name = "Readable \"UI\" name""#);
    assert_eq!(source_mods::mod_name(&root), "Readable \"UI\" name");
    write(&root, "descriptor.mod", "name = {");
    assert_eq!(source_mods::mod_name(&root), "Folder Name");
}

#[test]
fn scan_disables_new_mods_and_refreshes_the_snapshot_with_preserved_choices() {
    let fixture = fixture();
    let game = fixture.0.join("game");
    let user_data = fixture.0.join("user");
    let first = user_data.join("mod/first");
    let second = user_data.join("mod/second");
    for (root, x) in [(&first, 11), (&second, 23)] {
        write(
            root,
            "interface/example.gui",
            &format!("iconType = {{ name = test position = {{ x = {x} y = 0 }} }}"),
        );
    }
    descriptor(&user_data, "mod/first.mod", &first, "");
    descriptor(&user_data, "mod/second.mod", &second, "");
    export(&user_data, &["mod/first.mod"]);
    write(
        &game,
        "launcher-settings.json",
        &serde_json::json!({ "rawVersion": "v4.5.2", "gameDataPath": user_data }).to_string(),
    );
    let control = Control::default();
    let mut profile = Profile::default();
    let initial = compiler::scan_enabled_mods(&game, &profile, &control, &mut |_| {}).unwrap();
    let first = first.canonicalize().unwrap();
    let second = second.canonicalize().unwrap();
    assert_eq!(initial.plan.game.source_mods, Vec::<PathBuf>::new());
    assert!(initial.disabled_source_mods.contains(&first));
    assert!(initial.changes.is_none());
    assert!(profile.auto_source_mods.is_none());
    profile.auto_source_mods = Some(initial.settings);
    profile.disabled_source_mods = initial.disabled_source_mods;
    profile.disabled_source_mods.remove(&first);
    profile
        .source_mods
        .push(fixture.0.join("not-enabled-manual-source"));
    export(&user_data, &["mod/first.mod", "mod/second.mod"]);
    let updated = compiler::scan_enabled_mods(&game, &profile, &control, &mut |_| {}).unwrap();
    assert_eq!(
        updated.plan.game.source_mods.as_slice(),
        std::slice::from_ref(&first)
    );
    assert!(updated.plan.sources["interface/example.gui"].contains("x = 11"));
    assert!(updated.disabled_source_mods.contains(&second));
    assert!(!updated.disabled_source_mods.contains(&first));
    assert!(updated.removed_manual_sources);
    assert_eq!(
        updated.settings.source_mods,
        [first.clone(), second.clone()]
    );
    let changes = updated.changes.unwrap();
    assert_eq!(changes.saved.as_slice(), std::slice::from_ref(&first));
    assert_eq!(changes.current, [first.clone(), second.clone()]);
    profile.auto_source_mods = Some(updated.settings);
    profile.disabled_source_mods = updated.disabled_source_mods;
    profile.source_mods.clear();
    let output = fixture.0.join("compiled");
    build_mod(&updated.plan, &profile, &output).unwrap();
    export(&user_data, &["mod/second.mod", "mod/first.mod"]);
    assert!(build_mod(&updated.plan, &profile, &output).is_err());
    let reordered = compiler::scan_enabled_mods(&game, &profile, &control, &mut |_| {}).unwrap();
    assert_eq!(reordered.settings.source_mods, [second, first]);
    assert!(reordered.changes.is_some());
    export(&user_data, &[]);
    let empty = compiler::scan_enabled_mods(&game, &profile, &control, &mut |_| {}).unwrap();
    assert_eq!(empty.plan.game.source_mods, Vec::<PathBuf>::new());
    assert_eq!(empty.settings.source_mods, Vec::<PathBuf>::new());
    assert!(empty.changes.is_some());
}
