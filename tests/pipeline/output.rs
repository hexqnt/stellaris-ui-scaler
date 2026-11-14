use super::*;

#[test]
fn game_versions_warn_without_blocking_scan_or_build() {
    let fixture = fixture();
    let game = fixture.0.join("game");
    let control = Control::default();
    let profile = Profile::default();
    let output = fixture.0.join("mod");
    for (version, should_warn) in [
        (Some("v4.5.2"), false),
        (Some("v4.5.4"), false),
        (Some("4.5.100"), false),
        (Some("v4.4.9"), true),
        (Some("v4.6.0"), true),
        (Some("v5.5.0"), true),
        (Some("v4.50.0"), true),
        (Some("v4.5.invalid"), true),
        (Some("v4.5.4.extra"), true),
        (None, true),
    ] {
        if let Some(version) = version {
            write(
                &game,
                "launcher-settings.json",
                &serde_json::json!({ "rawVersion": version }).to_string(),
            );
        } else {
            fs::remove_file(game.join("launcher-settings.json")).unwrap();
        }
        let plan = compiler::plan(&game, &control, &mut |_| {}).unwrap();
        assert_eq!(plan.version_warning().is_some(), should_warn, "{version:?}");
        let report = compiler::build(&plan, &profile, &output, &control, &mut |_| {}).unwrap();
        assert_eq!(
            report
                .diagnostics
                .iter()
                .filter(|d| d.rule == "game-version")
                .count(),
            usize::from(should_warn),
            "{version:?}"
        );
        let descriptor = fs::read_to_string(output.join("descriptor.mod")).unwrap();
        assert!(descriptor.contains(&format!(
            "supported_version=\"{}\"",
            version.unwrap_or("unknown")
        )));
        assert!(report.validation.errors.is_empty());
    }
}

#[test]
fn mod_picture_is_saved_converted_replaced_and_removed() {
    let fixture = fixture();
    let control = Control::default();
    let plan = compiler::plan(&fixture.0.join("game"), &control, &mut |_| {}).unwrap();
    let output = fixture.0.join("mod");
    let source = fixture.0.join("cover.bmp");
    let image = image::RgbaImage::from_pixel(7, 5, image::Rgba([20, 40, 60, 255]));
    image.save(&source).unwrap();
    let profile = Profile {
        picture: Some(source.clone()),
        ..Profile::default()
    };
    let profile_path = fixture.0.join("profile.toml");
    profile.save(&profile_path).unwrap();
    let mut profile = Profile::load(&profile_path).unwrap();
    assert_eq!(profile.picture.as_ref(), Some(&source));
    for color in [[20, 40, 60, 255], [80, 100, 120, 255]] {
        let image = image::RgbaImage::from_pixel(7, 5, image::Rgba(color));
        image.save(&source).unwrap();
        compiler::build(&plan, &profile, &output, &control, &mut |_| {}).unwrap();
        assert_eq!(
            image::open(output.join("thumbnail.png"))
                .unwrap()
                .into_rgba8(),
            image
        );
        for path in [output.join("descriptor.mod"), fixture.0.join("mod.mod")] {
            let text = fs::read_to_string(path).unwrap();
            let document = Document::parse(&text).unwrap();
            assert_eq!(
                document
                    .nodes
                    .iter()
                    .find(|node| node.key == Some("picture"))
                    .unwrap()
                    .atom(),
                Some("thumbnail.png")
            );
        }
        let manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(output.join(compiler::manifest_name())).unwrap())
                .unwrap();
        assert!(manifest["files"]["thumbnail.png"].is_string());
    }

    let previous = fs::read(output.join("thumbnail.png")).unwrap();
    for invalid in [None, Some("not an image")] {
        if let Some(text) = invalid {
            fs::write(&source, text).unwrap();
        } else {
            fs::remove_file(&source).unwrap();
        }
        let error = compiler::build(&plan, &profile, &output, &control, &mut |_| {}).unwrap_err();
        assert!(format!("{error:#}").contains("mod picture"));
        assert_eq!(fs::read(output.join("thumbnail.png")).unwrap(), previous);
    }

    profile.picture = None;
    compiler::build(&plan, &profile, &output, &control, &mut |_| {}).unwrap();
    assert!(!output.join("thumbnail.png").exists());
    for path in [output.join("descriptor.mod"), fixture.0.join("mod.mod")] {
        assert!(!fs::read_to_string(path).unwrap().contains("picture="));
    }
}

#[test]
fn descriptors_include_mod_version_and_exact_game_version() {
    let fixture = fixture();
    let control = Control::default();
    let plan = compiler::plan(&fixture.0.join("game"), &control, &mut |_| {}).unwrap();
    let output = fixture.0.join("mod");
    let profile = Profile {
        mod_version: "2.3.0-beta.1".into(),
        ..Profile::default()
    };
    let profile_path = fixture.0.join("profile.toml");
    profile.save(&profile_path).unwrap();
    let profile = Profile::load(&profile_path).unwrap();
    compiler::build(&plan, &profile, &output, &control, &mut |_| {}).unwrap();

    let descriptor = fs::read_to_string(output.join("descriptor.mod")).unwrap();
    let document = Document::parse(&descriptor).unwrap();
    for (key, expected) in [
        ("name", profile.name.as_str()),
        ("version", "2.3.0-beta.1"),
        ("supported_version", "v4.5.2"),
    ] {
        let values: Vec<_> = document
            .nodes
            .iter()
            .filter(|node| node.key == Some(key))
            .collect();
        assert_eq!(values.len(), 1, "{key}");
        assert_eq!(values[0].atom(), Some(expected), "{key}");
    }
    let launcher = fs::read_to_string(fixture.0.join("mod.mod")).unwrap();
    let path_line = launcher.strip_prefix(&descriptor).unwrap();
    let document = Document::parse(path_line).unwrap();
    assert_eq!(document.nodes.len(), 1);
    assert_eq!(document.nodes[0].key, Some("path"));
    let path = document.nodes[0].atom().unwrap();
    assert!(!path.contains('\\'));
    assert_eq!(
        Path::new(path).canonicalize().unwrap(),
        output.canonicalize().unwrap()
    );
}

#[test]
fn supported_version_override_is_saved_and_does_not_change_asset_version() {
    let fixture = fixture();
    let control = Control::default();
    let plan = compiler::plan(&fixture.0.join("game"), &control, &mut |_| {}).unwrap();
    let output = fixture.0.join("mod");
    let profile_path = fixture.0.join("profile.toml");
    let mut profile = Profile {
        supported_version: Some("v4.5.*".into()),
        ..Profile::default()
    };
    for expected in ["v4.5.*", "v4.5.2"] {
        profile.save(&profile_path).unwrap();
        profile = Profile::load(&profile_path).unwrap();
        compiler::build(&plan, &profile, &output, &control, &mut |_| {}).unwrap();
        for path in [output.join("descriptor.mod"), fixture.0.join("mod.mod")] {
            let text = fs::read_to_string(path).unwrap();
            let document = Document::parse(&text).unwrap();
            assert_eq!(
                document
                    .nodes
                    .iter()
                    .find(|node| node.key == Some("supported_version"))
                    .unwrap()
                    .atom(),
                Some(expected)
            );
        }
        let manifest: compiler::Manifest =
            serde_json::from_slice(&fs::read(output.join(compiler::manifest_name())).unwrap())
                .unwrap();
        assert_eq!(manifest.game_version, "v4.5.2");
        profile.supported_version = None;
    }
}

#[test]
fn build_is_reproducible_cached_and_preserves_user_files() {
    let fixture = fixture();
    let control = Control::default();
    let plan = compiler::plan(&fixture.0.join("game"), &control, &mut |_| {}).unwrap();
    let output = fixture.0.join("mod");
    let profile = Profile::default();
    let report = compiler::build(&plan, &profile, &output, &control, &mut |_| {}).unwrap();
    assert_eq!(report.textures, 1);
    assert_eq!(report.cache_hits, 0);
    assert!(report.validation.errors.is_empty());
    let manifest = fs::read(output.join(compiler::manifest_name())).unwrap();
    write(&output, "user-notes.txt", "keep me");
    let report = compiler::build(&plan, &profile, &output, &control, &mut |_| {}).unwrap();
    assert_eq!(report.cache_hits, 1);
    assert_eq!(
        fs::read(output.join(compiler::manifest_name())).unwrap(),
        manifest
    );
    assert_eq!(
        fs::read_to_string(output.join("user-notes.txt")).unwrap(),
        "keep me"
    );
    let cancelled = Control::default();
    cancelled.cancel();
    assert!(compiler::build(&plan, &profile, &output, &cancelled, &mut |_| {}).is_err());
    assert_eq!(
        fs::read(output.join(compiler::manifest_name())).unwrap(),
        manifest
    );
    fs::remove_file(fixture.0.join("game/gfx/test.png")).unwrap();
    assert!(compiler::build(&plan, &profile, &output, &control, &mut |_| {}).is_err());
    assert_eq!(
        fs::read(output.join(compiler::manifest_name())).unwrap(),
        manifest
    );
}

#[test]
fn corrupted_cache_entries_are_rebuilt_and_published_without_temporary_files() {
    let fixture = fixture();
    let control = Control::default();
    let plan = compiler::plan(&fixture.0.join("game"), &control, &mut |_| {}).unwrap();
    let profile = Profile::default();
    let output = fixture.0.join("mod");
    compiler::build(&plan, &profile, &output, &control, &mut |_| {}).unwrap();
    let cache_root = fixture.0.join(".ui-scaler-cache");
    let entries = stellaris_ui_scaler::assets::files(&cache_root).unwrap();
    assert_eq!(entries.len(), 1);
    let cached = &entries[0];
    let expected = fs::read(cached).unwrap();
    for wrong_dimensions in [false, true] {
        if wrong_dimensions {
            image::RgbaImage::new(1, 1).save(cached).unwrap();
        } else {
            fs::write(cached, b"incomplete PNG").unwrap();
        }
        let report = compiler::build(&plan, &profile, &output, &control, &mut |_| {}).unwrap();
        assert_eq!(report.cache_hits, 0);
        assert_eq!(fs::read(cached).unwrap(), expected);
        assert_eq!(
            fs::read(output.join("gfx/test_scaled.png")).unwrap(),
            expected
        );
        assert_eq!(
            stellaris_ui_scaler::assets::files(&cache_root).unwrap(),
            entries
        );
    }
    let report = compiler::build(&plan, &profile, &output, &control, &mut |_| {}).unwrap();
    assert_eq!(report.cache_hits, 1);
    assert!(!fs::read_dir(&fixture.0).unwrap().any(|entry| {
        entry
            .unwrap()
            .path()
            .extension()
            .is_some_and(|extension| extension == "tmp")
    }));
}

#[test]
fn rebuild_preserves_large_user_files_and_records_generated_content_hashes() {
    use std::io::Write;

    let fixture = fixture();
    let control = Control::default();
    let plan = compiler::plan(&fixture.0.join("game"), &control, &mut |_| {}).unwrap();
    let output = fixture.0.join("mod");
    let profile = Profile::default();
    compiler::build(&plan, &profile, &output, &control, &mut |_| {}).unwrap();
    let user_path = output.join("user/archive.bin");
    fs::create_dir(user_path.parent().unwrap()).unwrap();
    let mut file = fs::File::create(&user_path).unwrap();
    file.set_len(1024 * 1024 + 13).unwrap();
    file.write_all(b"User data").unwrap();
    drop(file);
    let original = fs::read(&user_path).unwrap();

    compiler::build(&plan, &profile, &output, &control, &mut |_| {}).unwrap();
    assert_eq!(fs::read(&user_path).unwrap(), original);
    let manifest: compiler::Manifest =
        serde_json::from_slice(&fs::read(output.join(compiler::manifest_name())).unwrap()).unwrap();
    assert!(!manifest.files.contains_key("user/archive.bin"));
    for (path, expected) in manifest.files {
        assert!(!path.contains('\\'), "{path}");
        safe_relative(&path).unwrap();
        assert_eq!(
            blake3::hash(&fs::read(output.join(&path)).unwrap())
                .to_hex()
                .as_str(),
            expected,
            "{path}"
        );
    }
}

#[test]
fn rebuild_preserves_user_file_case_and_inherited_source_problems() {
    let fixture = fixture();
    let game = fixture.0.join("game");
    let source = r#"spriteTypes = {
        spriteType = { name = GFX_missing texturefile = "gfx/missing.dds" }
        spriteType = { name = GFX_duplicate texturefile = "gfx/test.png" noOfFrames = 2 noOfFrames = 2 }
    }"#;
    write(&game, "interface/inherited.gfx", source);
    let control = Control::default();
    let plan = compiler::plan(&game, &control, &mut |_| {}).unwrap();
    let output = fixture.0.join("Scaled UI");
    let profile = Profile::default();
    compiler::build(&plan, &profile, &output, &control, &mut |_| {}).unwrap();
    write(&output, "User Notes/Read Me.txt", "Keep my case");
    let report = compiler::build(&plan, &profile, &output, &control, &mut |_| {}).unwrap();
    assert!(report.validation.errors.is_empty());
    assert_eq!(report.validation.inherited_missing.len(), 1);
    assert_eq!(
        report.validation.inherited_missing[0].path,
        "interface/inherited.gfx"
    );
    assert_eq!(
        fs::read_to_string(output.join("User Notes/Read Me.txt")).unwrap(),
        "Keep my case"
    );
    let note = fs::read_dir(output.join("User Notes"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap();
    assert_eq!(note.file_name(), "Read Me.txt");
}

#[cfg(unix)]
#[test]
fn invalid_utf8_output_names_are_rejected_without_creating_a_lossy_path() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};

    let fixture = fixture();
    let control = Control::default();
    let plan = compiler::plan(&fixture.0.join("game"), &control, &mut |_| {}).unwrap();
    let output = fixture.0.join(OsString::from_vec(b"invalid-\xff".to_vec()));
    let error =
        compiler::build(&plan, &Profile::default(), &output, &control, &mut |_| {}).unwrap_err();
    assert!(error.to_string().contains("UTF-8"));
    assert!(!output.exists());
    assert!(!Path::new(output.to_string_lossy().as_ref()).exists());
}

#[test]
fn incremental_build_rehashes_content_and_removes_only_stale_owned_files() {
    let fixture = fixture();
    let control = Control::default();
    let profile = Profile::default();
    let game = fixture.0.join("game");
    let output = fixture.0.join("mod");
    let plan = compiler::plan(&game, &control, &mut |_| {}).unwrap();
    compiler::build(&plan, &profile, &output, &control, &mut |_| {}).unwrap();
    let before = fs::read(output.join("gfx/test_scaled.png")).unwrap();
    let mut image = image::open(game.join("gfx/test.png")).unwrap().to_rgba8();
    image.put_pixel(0, 0, image::Rgba([0, 0, 255, 255]));
    image.save(game.join("gfx/test.png")).unwrap();
    let report = compiler::build(&plan, &profile, &output, &control, &mut |_| {}).unwrap();
    assert_eq!(report.cache_hits, 0);
    assert_ne!(
        before,
        fs::read(output.join("gfx/test_scaled.png")).unwrap()
    );
    write(&output, "user/data.txt", "retained");
    write(&game, "interface/example.gfx", "spriteTypes = {}");
    let plan = compiler::plan(&game, &control, &mut |_| {}).unwrap();
    compiler::build(&plan, &profile, &output, &control, &mut |_| {}).unwrap();
    assert!(!output.join("gfx/test_scaled.png").exists());
    assert!(output.join("user/data.txt").exists());
}

#[test]
fn failed_publication_keeps_previous_output_and_launcher() {
    let fixture = fixture();
    let control = Control::default();
    let profile = Profile::default();
    let output = fixture.0.join("mod");
    let plan = compiler::plan(&fixture.0.join("game"), &control, &mut |_| {}).unwrap();
    compiler::build(&plan, &profile, &output, &control, &mut |_| {}).unwrap();
    let manifest = fs::read(output.join(compiler::manifest_name())).unwrap();
    for descriptor in [
        "another mod".to_owned(),
        format!("# path=\"{}\"\npath=\"/another/mod\"\n", output.display()),
    ] {
        fs::write(fixture.0.join("mod.mod"), &descriptor).unwrap();
        assert!(compiler::build(&plan, &profile, &output, &control, &mut |_| {}).is_err());
        assert_eq!(
            fs::read(output.join(compiler::manifest_name())).unwrap(),
            manifest
        );
        assert_eq!(
            fs::read_to_string(fixture.0.join("mod.mod")).unwrap(),
            descriptor
        );
    }
}

#[test]
fn duplicate_output_names_are_rejected_before_writing() {
    let fixture = fixture();
    let root = fixture.0.join("game");
    let output = fixture.0.join("mod");
    image::open(root.join("gfx/test.png"))
        .unwrap()
        .save(root.join("gfx/test.tga"))
        .unwrap();
    write(
        &root,
        "interface/second.gfx",
        "spriteTypes = { spriteType = { name = \"GFX_other\" texturefile = \"gfx/test.tga\" noOfFrames = 2 } }",
    );
    let plan = compiler::plan(&root, &Control::default(), &mut |_| {}).unwrap();
    assert_eq!(plan.textures.len(), 2);
    let error = compiler::build(
        &plan,
        &Profile::default(),
        &output,
        &Control::default(),
        &mut |_| {},
    )
    .unwrap_err();
    assert!(error.to_string().contains("collision"));
    assert!(!output.exists());
}
