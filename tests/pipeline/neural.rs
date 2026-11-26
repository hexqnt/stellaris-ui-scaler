use super::*;

fn options() -> TextureOptions {
    TextureOptions {
        algorithm: Algorithm::RealPlksr2x,
        sharpening: 0.0,
        ..Default::default()
    }
}

#[test]
fn realplksr_configuration_roundtrips_and_downsampling_does_not_require_a_model() {
    let profile = Profile {
        textures: TextureOptions {
            independent_frames: false,
            ..options()
        },
        auto_sharpening: true,
        ..Default::default()
    };
    let encoded = toml::to_string(&profile).unwrap();
    assert!(encoded.contains("realplksr_2x"));
    let decoded: Profile = toml::from_str(&encoded).unwrap();
    let processing =
        decoded.texture_processing("gfx/test.png", Category::Icon, Some((17, 13)), None);
    assert_eq!(processing.options.algorithm, Algorithm::RealPlksr2x);
    assert!(processing.options.independent_frames);
    assert!(!decoded.needs_pixel_analysis("gfx/test.png", Category::Icon, Some((17, 13))));
    let source = image::RgbaImage::from_pixel(17, 13, image::Rgba([90, 120, 200, 128]));
    let expected = textures::resize(
        &source,
        9,
        7,
        &TextureOptions {
            algorithm: Algorithm::Lanczos3,
            ..options()
        },
    )
    .unwrap();
    assert_eq!(
        textures::resize(&source, 9, 7, &options()).unwrap(),
        expected
    );
    let transparent = image::RgbaImage::from_pixel(5, 3, image::Rgba([255, 0, 255, 0]));
    assert!(
        textures::resize(&transparent, 10, 6, &options())
            .unwrap()
            .pixels()
            .all(|p| p.0 == [0; 4])
    );
}

#[test]
fn realplksr_cold_build_requires_a_model_even_with_classical_results_in_cache() {
    let fixture = fixture();
    let root = fixture.0.join("game");
    let output = fixture.0.join("mod");
    let run = |algorithm| {
        std::process::Command::new(env!("CARGO_BIN_EXE_stellaris-ui-scaler"))
            .args(["build", "--game"])
            .arg(&root)
            .args(["--scale", "2", "--algorithm", algorithm, "--output"])
            .arg(&output)
            .env(
                "STELLARIS_UI_SCALER_AI_DIR",
                fixture.0.join("missing-model"),
            )
            .output()
            .unwrap()
    };
    let result = run("realplksr-2x");
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("Cannot read RealPLKSR model"));
    assert!(!output.exists());

    let result = run("lanczos3");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let manifest = fs::read(output.join(compiler::manifest_name())).unwrap();
    let result = run("realplksr-2x");
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("Cannot read RealPLKSR model"));
    assert_eq!(
        fs::read(output.join(compiler::manifest_name())).unwrap(),
        manifest
    );
}

#[test]
#[ignore = "Requires the resources prepared by tools/prepare_ai.py"]
fn realplksr_build_matches_reference_pixels_and_cached_build_needs_no_model() {
    let fixture = fixture();
    let root = fixture.0.join("game");
    let source = image::RgbaImage::from_fn(17, 13, |x, y| {
        image::Rgba([
            u8::try_from((x * 17 + y * 3) % 256).unwrap(),
            u8::try_from((x * 5 + y * 19) % 256).unwrap(),
            u8::try_from((x * 11 + y * 7) % 256).unwrap(),
            255,
        ])
    });
    source.save(root.join("gfx/test.png")).unwrap();
    write(
        &root,
        "interface/example.gfx",
        "spriteTypes = { spriteType = { name = GFX_test texturefile = \"gfx/test.png\" } }",
    );
    let profile = Profile {
        scale: Scale::try_from(2.0).unwrap(),
        textures: options(),
        ..Default::default()
    };
    let plan = scan_game(&root).unwrap();
    let output = fixture.0.join("mod");
    let report = build_mod(&plan, &profile, &output).unwrap();
    assert_eq!(report.textures, 1);
    assert_eq!(report.cache_hits, 0);
    let png = output.join("gfx/test_scaled.png");
    let bytes = fs::read(&png).unwrap();
    let image = textures::decode(&bytes).unwrap();
    assert_eq!(image.dimensions(), (34, 26));
    // Независимый эталон Python ONNX Runtime; проверяем именно результат сборки.
    for ((x, y), expected) in [
        ((0, 0), [0, 0, 0]),
        ((33, 0), [15, 79, 170]),
        ((0, 25), [39, 219, 83]),
        ((33, 25), [67, 71, 0]),
        ((16, 12), [151, 151, 127]),
        ((9, 8), [82, 96, 73]),
    ] {
        assert!(
            image.get_pixel(x, y).0[..3]
                .iter()
                .zip(expected)
                .all(|(&a, b)| a.abs_diff(b) <= 1)
        );
    }
    let classical = textures::resize(
        &source,
        34,
        26,
        &TextureOptions {
            algorithm: Algorithm::Lanczos3,
            ..options()
        },
    )
    .unwrap();
    assert_ne!(image, classical);

    let saved = fixture.0.join("profile.toml");
    profile.save(&saved).unwrap();
    // Новый процесс исключает переиспользование уже загруженной сессии модели.
    let cached = std::process::Command::new(env!("CARGO_BIN_EXE_stellaris-ui-scaler"))
        .args(["build", "--game"])
        .arg(&root)
        .arg("--profile")
        .arg(&saved)
        .arg("--output")
        .arg(&output)
        .env(
            "STELLARIS_UI_SCALER_AI_DIR",
            fixture.0.join("missing-model"),
        )
        .output()
        .unwrap();
    assert!(
        cached.status.success(),
        "{}",
        String::from_utf8_lossy(&cached.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&cached.stdout).unwrap();
    assert_eq!(report["cache_hits"], 1);
    assert_eq!(fs::read(png).unwrap(), bytes);
}

#[test]
#[ignore = "Requires the resources prepared by tools/prepare_ai.py"]
fn realplksr_frames_alpha_and_arbitrary_scales_match_individual_processing() {
    use image::GenericImageView;

    let source = image::RgbaImage::from_fn(17, 13, |x, y| {
        if x < 8 {
            image::Rgba([90, 120, 200, u8::try_from((x + y) * 12).unwrap()])
        } else {
            image::Rgba([200, 90, 120, 255])
        }
    });
    let options = TextureOptions {
        independent_frames: false,
        ..options()
    };
    for scale in [1.5, 2.0, 2.5] {
        let scale = Scale::try_from(scale).unwrap();
        let output = textures::process(&source, scale, 2, &options).unwrap();
        let target_width = scale.dimension(8);
        assert_eq!(output.dimensions(), (target_width * 2, scale.dimension(13)));
        for frame in 0..2 {
            let input = source.view(frame * 8, 0, 8, 13).to_image();
            let expected =
                textures::resize(&input, target_width, scale.dimension(13), &options).unwrap();
            assert_eq!(
                output
                    .view(frame * target_width, 0, target_width, output.height())
                    .to_image(),
                expected
            );
        }
    }
    let mut hidden_a = source.view(0, 0, 8, 13).to_image();
    let mut hidden_b = hidden_a.clone();
    hidden_a.put_pixel(0, 0, image::Rgba([255, 0, 0, 0]));
    hidden_b.put_pixel(0, 0, image::Rgba([0, 255, 0, 0]));
    let a = textures::resize(&hidden_a, 16, 26, &options).unwrap();
    let b = textures::resize(&hidden_b, 16, 26, &options).unwrap();
    assert_eq!(a, b);
    let alpha = image::imageops::resize(&hidden_a, 16, 26, image::imageops::FilterType::Lanczos3);
    assert!(a.pixels().zip(alpha.pixels()).all(|(a, b)| a[3] == b[3]));
    let periodic = textures::periodic(&source, Scale::try_from(1.5).unwrap(), 2, &options).unwrap();
    assert_eq!(periodic.dimensions(), (24, 20));
}

#[test]
#[ignore = "Requires the resources prepared by tools/prepare_ai.py"]
fn realplksr_preview_build_and_cache_produce_identical_outputs() {
    let fixture = fixture();
    let root = fixture.0.join("game");
    let control = Control::default();
    let plan = scan_game(&root).unwrap();
    let asset = &plan.textures[0];
    let profile = Profile {
        scale: Scale::try_from(1.5).unwrap(),
        textures: options(),
        ..Default::default()
    };
    let preview = compiler::preview(&plan, &profile, asset).unwrap();
    let candidates =
        compiler::preview_candidates(&plan, &profile, asset, &[Algorithm::RealPlksr2x], &control)
            .unwrap();
    assert_eq!(candidates[1].1, preview);
    let output = fixture.0.join("mod");
    let build = || build_mod(&plan, &profile, &output).unwrap();
    assert_eq!(build().cache_hits, 0);
    let png = output.join(textures::output_name(&asset.path));
    assert_eq!(textures::decode(&fs::read(&png).unwrap()).unwrap(), preview);
    assert_eq!(build().cache_hits, plan.textures.len());
    let classical = Profile {
        textures: TextureOptions {
            algorithm: Algorithm::Lanczos3,
            ..options()
        },
        ..profile.clone()
    };
    let report = build_mod(&plan, &classical, &output).unwrap();
    assert_eq!(report.cache_hits, 0);
    assert_eq!(build().cache_hits, plan.textures.len());
    assert_eq!(textures::decode(&fs::read(&png).unwrap()).unwrap(), preview);

    let cli_output = fixture.0.join("cli-mod");
    let saved = fixture.0.join("profile.toml");
    Profile {
        auto_algorithm: true,
        ..classical
    }
    .save(&saved)
    .unwrap();
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_stellaris-ui-scaler"))
        .args(["build", "--game"])
        .arg(&root)
        .arg("--profile")
        .arg(&saved)
        .args(["--algorithm", "realplksr-2x", "--output"])
        .arg(&cli_output)
        .output()
        .unwrap();
    assert!(
        status.status.success(),
        "{}",
        String::from_utf8_lossy(&status.stderr)
    );
    let cli_png = cli_output.join(textures::output_name(&asset.path));
    assert_eq!(
        textures::decode(&fs::read(cli_png).unwrap()).unwrap(),
        preview
    );
}

#[test]
#[ignore = "Requires the resources prepared by tools/prepare_ai.py"]
fn realplksr_streaming_session_reuses_buffers_for_new_sources() {
    use compiler::{PreviewEvent, PreviewSession};
    use std::sync::mpsc;
    let fixture = fixture();
    let root = fixture.0.join("game");
    let control = Control::default();
    let plan = scan_game(&root).unwrap();
    let asset = &plan.textures[0];
    let profile = Profile {
        scale: Scale::try_from(1.5).unwrap(),
        textures: options(),
        ..Default::default()
    };
    let session = PreviewSession::default();
    for color in [[90, 120, 200, 255], [200, 40, 70, 128]] {
        let source = image::RgbaImage::from_pixel(asset.width, asset.height, image::Rgba(color));
        source.save(root.join(&asset.path)).unwrap();
        let (sender, receiver) = mpsc::channel();
        session
            .render(
                &plan,
                &profile,
                asset,
                &[Algorithm::RealPlksr2x, Algorithm::Lanczos3],
                &control,
                &|event| {
                    sender.send(event).unwrap();
                },
            )
            .unwrap();
        drop(sender);
        let results: Vec<_> = receiver
            .into_iter()
            .filter_map(|event| match event {
                PreviewEvent::Source { .. } => None,
                PreviewEvent::Candidate { index, image } => Some((index, image.unwrap())),
            })
            .collect();
        assert_eq!(results.len(), 2);
        for (index, actual) in results {
            let options = TextureOptions {
                algorithm: [Algorithm::RealPlksr2x, Algorithm::Lanczos3][index],
                ..options()
            };
            assert_eq!(
                actual,
                textures::process(&source, profile.scale, asset.frames, &options).unwrap()
            );
        }
    }
}

#[test]
#[ignore = "Requires the resources prepared by tools/prepare_ai.py"]
fn realplksr_parallel_build_matches_single_worker_and_reuses_cache() {
    let fixture = fixture();
    let root = fixture.0.join("game");
    let mut gfx = String::from("spriteTypes = {");
    for index in 0..8 {
        let path = format!("gfx/neural_{index}.png");
        let source = image::RgbaImage::from_fn(96, 96, |x, y| {
            image::Rgba([
                u8::try_from((x * 17 + y * 3 + index * 7) % 256).unwrap(),
                u8::try_from((x * 5 + y * 19) % 256).unwrap(),
                120,
                255,
            ])
        });
        source.save(root.join(&path)).unwrap();
        write!(
            gfx,
            "spriteType = {{ name = GFX_neural_{index} texturefile = \"{path}\" }}"
        )
        .unwrap();
    }
    gfx.push('}');
    write(&root, "interface/neural.gfx", &gfx);
    let plan = scan_game(&root).unwrap();
    let profile = Profile {
        scale: Scale::try_from(1.5).unwrap(),
        textures: options(),
        ..Default::default()
    };
    let mut expected = None;
    for threads in [1, 3, 8] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap();
        let output = fixture.0.join(format!("threads-{threads}/mod"));
        pool.install(|| {
            let started = std::time::Instant::now();
            let report = build_mod(&plan, &profile, &output).unwrap();
            eprintln!(
                "Neural build with {threads} workers: {:?}",
                started.elapsed()
            );
            assert_eq!(report.cache_hits, 0);
            let manifest = fs::read(output.join(compiler::manifest_name())).unwrap();
            if let Some(expected) = &expected {
                assert_eq!(&manifest, expected);
            } else {
                expected = Some(manifest);
            }
            let cached = build_mod(&plan, &profile, &output).unwrap();
            assert_eq!(cached.cache_hits, plan.textures.len());
        });
    }
}
