use super::{
    discovery_tests::{descriptor, export},
    *,
};
use stellaris_ui_scaler::config::AutoSourceMods;

fn cli(command: &str, game: &Path, profile: &Path) -> std::process::Command {
    let mut cli = std::process::Command::new(env!("CARGO_BIN_EXE_stellaris-ui-scaler"));
    cli.arg(command)
        .arg("--game")
        .arg(game)
        .arg("--profile")
        .arg(profile);
    cli
}

#[test]
fn cli_auto_sources_precede_profile_and_explicit_overrides() {
    let fixture = fixture();
    let game = fixture.0.join("game");
    let user_data = fixture.0.join("user");
    let automatic = user_data.join("mod/automatic");
    let configured = fixture.0.join("configured");
    let explicit = fixture.0.join("explicit");
    for (root, x) in [(&automatic, 11), (&configured, 23), (&explicit, 35)] {
        write(
            root,
            "interface/example.gui",
            &format!("iconType = {{ name = test position = {{ x = {x} y = 0 }} }}"),
        );
    }
    descriptor(&user_data, "mod/automatic.mod", &automatic, "");
    export(&user_data, &["mod/automatic.mod"]);
    let profile_path = fixture.0.join("profile.toml");
    Profile {
        scale: Scale::try_from(2.0).unwrap(),
        source_mods: vec![configured.clone()],
        ..Profile::default()
    }
    .save(&profile_path)
    .unwrap();
    let output = fixture.0.join("compiled");
    let result = cli("build", &game, &profile_path)
        .arg("--auto-source-mods")
        .arg("--user-data-dir")
        .arg(&user_data)
        .arg("--source-mod")
        .arg(&explicit)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(output.join(compiler::manifest_name())).unwrap()).unwrap();
    assert_eq!(
        manifest["source_mods"],
        serde_json::json!([
            automatic.canonicalize().unwrap(),
            configured.canonicalize().unwrap(),
            explicit.canonicalize().unwrap()
        ])
    );
    assert_eq!(
        manifest["profile"]["source_mods"],
        serde_json::json!([configured, explicit])
    );
    assert_eq!(
        Profile::load(&profile_path).unwrap().source_mods,
        [configured]
    );
    let gui = fs::read_to_string(output.join("interface/example.gui")).unwrap();
    let document = Document::parse(&gui).unwrap();
    assert_eq!(number(&document.nodes[0], "position", "x"), 70.0);
}

#[test]
fn cli_uses_automatic_settings_from_profile_and_can_disable_them() {
    let fixture = fixture();
    let game = fixture.0.join("game");
    let user_data = fixture.0.join("user");
    let root = user_data.join("mod/automatic");
    write(
        &root,
        "interface/example.gui",
        "iconType = { name = test position = { x = 11 y = 0 } }",
    );
    descriptor(&user_data, "mod/automatic.mod", &root, "");
    export(&user_data, &["mod/automatic.mod"]);
    let profile_path = fixture.0.join("profile.toml");
    Profile {
        auto_source_mods: Some(AutoSourceMods {
            user_data_dir: Some(user_data.clone()),
            source_mods: vec![root.canonicalize().unwrap()],
        }),
        ..Profile::default()
    }
    .save(&profile_path)
    .unwrap();
    for automatic in [true, false] {
        if !automatic {
            fs::remove_file(user_data.join("dlc_load.json")).unwrap();
        }
        let output = fixture
            .0
            .join(if automatic { "automatic" } else { "vanilla" });
        let mut command = cli("build", &game, &profile_path);
        command.arg("--output").arg(&output);
        if !automatic {
            command.arg("--no-auto-source-mods");
        }
        let result = command.output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let manifest: compiler::Manifest =
            serde_json::from_slice(&fs::read(output.join(compiler::manifest_name())).unwrap())
                .unwrap();
        assert_eq!(manifest.profile.auto_source_mods.is_some(), automatic);
        assert_eq!(manifest.profile.source_mods, Vec::<PathBuf>::new());
        assert_eq!(
            manifest.source_mods,
            if automatic {
                vec![root.canonicalize().unwrap()]
            } else {
                Vec::new()
            }
        );
    }
}

#[test]
fn cli_sync_saves_the_current_list_and_failed_sync_preserves_the_profile() {
    let fixture = fixture();
    let game = fixture.0.join("game");
    let user_data = fixture.0.join("user");
    let root = user_data.join("mod/automatic");
    write(
        &root,
        "interface/example.gui",
        "iconType = { name = test position = { x = 11 y = 0 } }",
    );
    descriptor(&user_data, "mod/automatic.mod", &root, "");
    export(&user_data, &["mod/automatic.mod"]);
    let profile_path = fixture.0.join("profile.toml");
    Profile {
        scale: Scale::try_from(2.0).unwrap(),
        auto_source_mods: Some(AutoSourceMods {
            user_data_dir: Some(user_data.clone()),
            ..AutoSourceMods::default()
        }),
        ..Profile::default()
    }
    .save(&profile_path)
    .unwrap();
    let before = fs::read(&profile_path).unwrap();
    for command in ["scan", "build"] {
        let result = cli(command, &game, &profile_path)
            .arg("--output")
            .arg(fixture.0.join("rejected"))
            .output()
            .unwrap();
        assert!(!result.status.success());
        let message = String::from_utf8_lossy(&result.stderr);
        assert!(
            message.contains("Saved order:") && message.contains("Current order:"),
            "{message}"
        );
        assert!(!fixture.0.join("rejected").exists());
    }
    assert_eq!(fs::read(&profile_path).unwrap(), before);
    let run_sync = || {
        cli("sync-source-mods", &game, &profile_path)
            .output()
            .unwrap()
    };
    let result = run_sync();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let profile = Profile::load(&profile_path).unwrap();
    assert_eq!(profile.scale.get(), 2.0);
    assert_eq!(
        profile.auto_source_mods.as_ref().unwrap().source_mods,
        [root.canonicalize().unwrap()]
    );
    scan_game_with_profile(&game, &profile).unwrap();
    let saved = fs::read(&profile_path).unwrap();
    write(&user_data, "dlc_load.json", "broken export");
    assert!(!run_sync().status.success());
    assert_eq!(fs::read(&profile_path).unwrap(), saved);
}
