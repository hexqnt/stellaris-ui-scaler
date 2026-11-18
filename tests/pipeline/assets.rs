use super::*;

#[test]
fn gfx_mask_roles_are_recognized_without_misclassifying_shared_color_textures() {
    let fixture = fixture();
    let root = fixture.0.join("game");
    fs::copy(
        root.join("gfx/test.png"),
        root.join("gfx/animation_mask.png"),
    )
    .unwrap();
    fs::copy(root.join("gfx/test.png"), root.join("gfx/shared.png")).unwrap();
    write(
        &root,
        "interface/example.gfx",
        r#"spriteTypes = {
        spriteType = { name = "GFX_test" texturefile = "gfx/test.png" animationmaskfile = "gfx/animation_mask.png" masking_texture = "gfx/shared.png" }
        spriteType = { name = "GFX_other" texturefile = "gfx/shared.png" }
    }"#,
    );
    let plan = compiler::plan(&root, &Control::default(), &mut |_| {}).unwrap();
    let category = |path| {
        plan.textures
            .iter()
            .find(|asset| asset.path == path)
            .unwrap()
            .category
    };
    assert_eq!(category("gfx/animation_mask.png"), Category::Mask);
    assert_eq!(category("gfx/shared.png"), Category::Icon);
    assert_eq!(category("gfx/test.png"), Category::Icon);
}

#[test]
fn validation_reports_inherited_missing_without_repairing() {
    let fixture = fixture();
    let game = Installation::open(&fixture.0.join("game")).unwrap();
    let output = fixture.0.join("mod");
    write(
        &output,
        "interface/example.gfx",
        "spriteTypes = { spriteType = { name = \"test\" textureFile = \"gfx/no_scaled.png\" } }",
    );
    let before = fs::read(output.join("interface/example.gfx")).unwrap();
    let report = validation::validate(&game, &output).unwrap();
    assert!(!report.errors.is_empty());
    assert_eq!(
        fs::read(output.join("interface/example.gfx")).unwrap(),
        before
    );
}

#[test]
fn validation_rejects_missing_directories_and_regular_files() {
    let fixture = fixture();
    let game = Installation::open(&fixture.0.join("game")).unwrap();
    let output = fixture.0.join("mod");
    assert!(validation::validate(&game, &output).is_err());
    fs::write(&output, "not a directory").unwrap();
    assert!(validation::validate(&game, &output).is_err());
    assert_eq!(fs::read_to_string(output).unwrap(), "not a directory");
}

#[test]
fn validation_checks_uppercase_extensions_and_normalized_frame_references() {
    let fixture = fixture();
    let game = Installation::open(&fixture.0.join("game")).unwrap();
    let output = fixture.0.join("mod");
    write(
        &output,
        "interface/frame.GFX",
        r#"spriteType = { name = "frame" textureFile = "GFX\TEST_SCALED.PNG" noOfFrames = 2 }"#,
    );
    write(&output, "fonts/test.ASSET", "font = { name = test }");
    write(&output, "interface/broken.GUI", "broken = {");
    write(&output, "common/forbidden.txt", "");
    write(&output, "common-extra/allowed.txt", "");
    write(&output, "interface-extra/ignored.GFX", "ignored = {");
    fs::create_dir(output.join("gfx")).unwrap();
    image::RgbaImage::new(5, 2)
        .save(output.join("gfx/test_scaled.png"))
        .unwrap();
    let report = validation::validate(&game, &output).unwrap();
    assert_eq!(report.checked_files, 2);
    assert_eq!(report.errors.len(), 3);
    for rule in ["syntax", "frame-dimensions", "gameplay-data"] {
        assert_eq!(
            report
                .errors
                .iter()
                .filter(|error| error.rule == rule)
                .count(),
            1,
            "{rule}"
        );
    }
}

#[cfg(unix)]
#[test]
fn resource_walk_rejects_root_and_nested_symbolic_links() {
    use std::os::unix::fs::symlink;
    use stellaris_ui_scaler::assets::files;

    let temp = Temp::new();
    let root = temp.0.join("resources");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("data.txt"), "data").unwrap();
    let link = temp.0.join("link");
    symlink(&root, &link).unwrap();
    assert!(files(&link).is_err());
    let missing = temp.0.join("missing");
    assert_eq!(files(&missing).unwrap(), Vec::<PathBuf>::new());
    let dangling = temp.0.join("dangling");
    symlink(&missing, &dangling).unwrap();
    assert!(files(&dangling).is_err());
    for target in [&root, &root.join("data.txt")] {
        let nested = root.join("nested");
        symlink(target, &nested).unwrap();
        assert!(files(&root).is_err());
        fs::remove_file(nested).unwrap();
    }
    assert_eq!(files(&root).unwrap(), [root.join("data.txt")]);
}

#[test]
fn dlc_resources_are_resolved_without_extracting_archives() {
    use rayon::prelude::*;
    use std::io::Write;
    let fixture = fixture();
    let root = fixture.0.join("game");
    fs::create_dir_all(root.join("dlc/test")).unwrap();
    let file = fs::File::create(root.join("dlc/test/test.zip")).unwrap();
    let mut writer = zip::ZipWriter::new(file);
    for (name, contents) in [
        ("gfx/dlc/asset.txt", "DLC resource"),
        ("GFX/DLC/UPPER.TXT", "Uppercase resource"),
        (r"gfx\dlc\backslash.txt", "Backslash resource"),
        ("GFX/DLC/ФОН.TXT", "Unicode resource"),
        ("gfx/../outside.txt", "Unsafe resource"),
    ] {
        writer
            .start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(contents.as_bytes()).unwrap();
    }
    writer.finish().unwrap();
    let game = Installation::open(&root).unwrap();
    assert_eq!(game.archives, 1);
    assert_eq!(game.read("gfx/dlc/asset.txt").unwrap(), b"DLC resource");
    let cases = [
        ("gfx/dlc/upper.txt", "Uppercase resource"),
        ("GFX/DLC/UPPER.TXT", "Uppercase resource"),
        ("gfx/dlc/backslash.txt", "Backslash resource"),
        ("gfx/dlc/фон.txt", "Unicode resource"),
    ];
    for (path, expected) in cases {
        assert!(game.contains(path), "{path}");
        assert_eq!(game.text(path).unwrap(), expected);
    }
    assert!(!game.contains("gfx/../outside.txt"));
    assert!(game.read("gfx/../outside.txt").is_err());
    assert!(!root.join("gfx/dlc").exists());
    let cloned = game.clone();
    (0..64).into_par_iter().for_each(|index| {
        let installation = if index % 2 == 0 { &game } else { &cloned };
        let (path, expected) = cases[index % cases.len()];
        assert_eq!(installation.text(path).unwrap(), expected);
    });
}
