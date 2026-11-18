use super::*;

fn build_texture(
    plan: &compiler::BuildPlan,
    profile: &Profile,
    output: &Path,
    control: &Control,
    hits: usize,
) -> image::RgbaImage {
    let report = compiler::build(plan, profile, output, control, &mut |_| {}).unwrap();
    assert_eq!(report.cache_hits, hits);
    image::open(output.join("gfx/test_scaled.png"))
        .unwrap()
        .into_rgba8()
}

#[test]
fn mixed_atlas_preview_and_cache_share_the_fallback() {
    let fixture = fixture();
    let root = fixture.0.join("game");
    write(
        &root,
        "interface/example.gfx",
        r#"spriteTypes = { spriteType = { name = "GFX_test" textureFile = "gfx/test.png" noOfFrames = 3 } }"#,
    );
    let source = test_images::mixed_atlas([0, 1, 2]);
    source.save(root.join("gfx/test.png")).unwrap();
    let control = Control::default();
    let plan = compiler::plan(&root, &control, &mut |_| {}).unwrap();
    let asset = &plan.textures[0];
    let profile = Profile {
        auto_algorithm: true,
        auto_sharpening: true,
        scale: Scale::try_from(1.5).unwrap(),
        ..Profile::default()
    };
    let features = asset.analyze(&profile, &source).unwrap();
    let processing = asset.processing(&profile, features.as_ref());
    assert_eq!(processing.options.algorithm, Algorithm::CatmullRom);
    assert_eq!(processing.options.sharpening, 0.15);
    assert_eq!(
        processing.recommendation.unwrap().reason,
        "mixed sprite-frame structure; texture purpose"
    );
    let expected = textures::process(
        &source,
        profile.scale,
        3,
        &TextureOptions {
            algorithm: Algorithm::CatmullRom,
            sharpening: 0.15,
            ..profile.textures
        },
    )
    .unwrap();
    assert_eq!(compiler::preview(&plan, &profile, asset).unwrap(), expected);
    let candidates =
        compiler::preview_candidates(&plan, &profile, asset, &[Algorithm::CatmullRom], &control)
            .unwrap();
    assert_eq!(candidates[1].1, expected);
    let output = fixture.0.join("mod");
    for hits in [0, 1] {
        assert_eq!(
            build_texture(&plan, &profile, &output, &control, hits),
            expected
        );
    }
}

#[test]
fn unstructured_detail_limits_sharpening_and_preserves_explicit_rules() {
    let fixture = fixture();
    let root = fixture.0.join("game");
    write(
        &root,
        "interface/example.gfx",
        r#"spriteTypes = { spriteType = { name = "GFX_test" textureFile = "gfx/test.png" } }"#,
    );
    let source = test_images::unstructured();
    source.save(root.join("gfx/test.png")).unwrap();
    let control = Control::default();
    let plan = compiler::plan(&root, &control, &mut |_| {}).unwrap();
    let asset = &plan.textures[0];
    let mut profile = Profile {
        auto_algorithm: true,
        auto_sharpening: true,
        scale: Scale::try_from(1.5).unwrap(),
        ..Profile::default()
    };
    let features = asset.analyze(&profile, &source).unwrap();
    let processing = asset.processing(&profile, features.as_ref());
    assert_eq!(processing.options.algorithm, Algorithm::CatmullRom);
    assert_eq!(processing.options.sharpening, 0.15);
    assert_eq!(
        processing.recommendation.unwrap().reason,
        "unstructured low-contrast detail; texture purpose"
    );
    assert_eq!(
        compiler::preview(&plan, &profile, asset).unwrap(),
        textures::process(&source, profile.scale, 1, &processing.options).unwrap()
    );

    profile.assets.insert(
        asset.path.clone(),
        TextureRule {
            algorithm: Some(Algorithm::Lanczos3),
            sharpening: Some(0.7),
            ..Default::default()
        },
    );
    assert!(asset.analyze(&profile, &source).unwrap().is_none());
    let options = asset.options(&profile);
    assert_eq!(options.algorithm, Algorithm::Lanczos3);
    assert_eq!(options.sharpening, 0.7);
    assert_eq!(
        compiler::preview(&plan, &profile, asset).unwrap(),
        textures::process(&source, profile.scale, 1, &options).unwrap()
    );
}

#[test]
fn candidate_sharpening_analyzes_pixels_even_when_the_current_filter_is_fixed() {
    let fixture = fixture();
    let root = fixture.0.join("game");
    let source = test_images::flat_graphic();
    source.save(root.join("gfx/test.png")).unwrap();
    let control = Control::default();
    let plan = compiler::plan(&root, &control, &mut |_| {}).unwrap();
    let asset = &plan.textures[0];
    let mut profile = Profile {
        auto_sharpening: true,
        scale: Scale::try_from(1.5).unwrap(),
        ..Profile::default()
    };
    profile.textures.algorithm = Algorithm::Bilinear;
    assert!(asset.analyze(&profile, &source).unwrap().is_none());
    let candidates = compiler::preview_candidates_with_analysis(
        &plan,
        &profile,
        asset,
        &[Algorithm::Lanczos3],
        &control,
    )
    .unwrap();
    assert!(candidates.features.is_some());
    profile.assets.insert(
        asset.path.clone(),
        TextureRule {
            algorithm: Some(Algorithm::Lanczos3),
            ..Default::default()
        },
    );
    assert_eq!(
        asset
            .processing(&profile, candidates.features.as_ref())
            .options
            .sharpening,
        0.15
    );
    assert_eq!(
        compiler::preview(&plan, &profile, asset).unwrap(),
        candidates.images[1].1
    );
}

#[test]
fn pixel_auto_preserves_preview_build_and_cache_policy() {
    let fixture = fixture();
    let root = fixture.0.join("game");
    let source = test_images::flat_graphic();
    source.save(root.join("gfx/test.png")).unwrap();
    let control = Control::default();
    let plan = compiler::plan(&root, &control, &mut |_| {}).unwrap();
    let asset = &plan.textures[0];
    let mut profile = Profile {
        auto_algorithm: true,
        auto_sharpening: true,
        scale: Scale::try_from(1.5).unwrap(),
        ..Profile::default()
    };
    assert_eq!(asset.options(&profile).algorithm, Algorithm::CatmullRom);
    let analysis = compiler::preview_candidates_with_analysis(
        &plan,
        &profile,
        asset,
        &[Algorithm::Bilinear],
        &control,
    )
    .unwrap();
    let decision = asset.processing(&profile, analysis.features.as_ref());
    assert_eq!(decision.options.algorithm, Algorithm::Mitchell);
    assert_eq!(decision.options.sharpening, 0.15);
    let auto = compiler::preview(&plan, &profile, asset).unwrap();
    assert_eq!(
        auto,
        textures::process(
            &source,
            profile.scale,
            2,
            &TextureOptions {
                algorithm: Algorithm::Mitchell,
                sharpening: 0.15,
                ..profile.textures
            }
        )
        .unwrap()
    );
    let output = fixture.0.join("mod");
    let build = |profile: &Profile, expected_hits| {
        build_texture(&plan, profile, &output, &control, expected_hits)
    };
    for expected_hits in [0, 1] {
        assert_eq!(build(&profile, expected_hits), auto);
    }

    // Ручной CatmullRom совпадает с предварительным Auto, но отличается от решения по пикселям.
    profile.assets.insert(
        asset.path.clone(),
        TextureRule {
            algorithm: Some(Algorithm::CatmullRom),
            ..Default::default()
        },
    );
    assert_ne!(build(&profile, 0), auto);
    assert_ne!(compiler::preview(&plan, &profile, asset).unwrap(), auto);

    // Явная резкость совпадает с глобальной, но запрещает автоматическое ограничение.
    profile.assets.insert(
        asset.path.clone(),
        TextureRule {
            sharpening: Some(profile.textures.sharpening),
            ..Default::default()
        },
    );
    let explicit_sharpening = compiler::preview(&plan, &profile, asset).unwrap();
    assert_ne!(explicit_sharpening, auto);
    assert_eq!(build(&profile, 0), explicit_sharpening);

    // Сохранённый кандидат должен совпадать со своим предпросмотром, включая Auto sharpening.
    profile.assets.insert(
        asset.path.clone(),
        TextureRule {
            algorithm: Some(Algorithm::Bilinear),
            ..Default::default()
        },
    );
    assert_eq!(build(&profile, 0), analysis.images[1].1);
    assert_eq!(build(&profile, 1), analysis.images[1].1);
}

#[test]
fn preview_entry_points_reject_invalid_profiles() {
    let fixture = fixture();
    let control = Control::default();
    let plan = compiler::plan(&fixture.0.join("game"), &control, &mut |_| {}).unwrap();
    let mut profile = Profile::default();
    profile.textures.sharpening = f32::NAN;
    let asset = &plan.textures[0];
    assert!(compiler::preview(&plan, &profile, asset).is_err());
    assert!(
        compiler::preview_candidates(&plan, &profile, asset, &Algorithm::ALL, &control).is_err()
    );
}

#[test]
fn gui_candidates_and_build_share_texture_transformation() {
    let fixture = fixture();
    let control = Control::default();
    let mut profile = Profile::default();
    profile.assets.insert(
        "gfx/test.png".into(),
        TextureRule {
            algorithm: Some(Algorithm::Mitchell),
            ..Default::default()
        },
    );
    let plan = compiler::plan(&fixture.0.join("game"), &control, &mut |_| {}).unwrap();
    let output = fixture.0.join("mod");
    compiler::build(&plan, &profile, &output, &control, &mut |_| {}).unwrap();
    let candidate = compiler::preview_candidates(
        &plan,
        &profile,
        &plan.textures[0],
        &[Algorithm::Mitchell],
        &control,
    )
    .unwrap();
    let built = image::open(output.join("gfx/test_scaled.png"))
        .unwrap()
        .to_rgba8();
    assert_eq!(built, candidate[1].1);
}

#[test]
fn auto_algorithm_matches_preview_build_and_cache() {
    let fixture = fixture();
    let root = fixture.0.join("game");
    // Широкий атлас с маленькими кадрами должен считаться UI, а не фоном.
    write(
        &root,
        "interface/example.gfx",
        r#"spriteTypes = { spriteType = { name = "GFX_test" textureFile = "gfx/test.png" noOfFrames = 100 } }"#,
    );
    let source = image::RgbaImage::from_fn(400, 4, |x, y| {
        if (x + y) % 3 == 0 {
            image::Rgba([255, 255, 255, 255])
        } else {
            image::Rgba([0, 0, 0, 255])
        }
    });
    source.save(root.join("gfx/test.png")).unwrap();
    let control = Control::default();
    let mut profile = Profile {
        auto_algorithm: true,
        scale: Scale::try_from(1.5).unwrap(),
        ..Profile::default()
    };
    let plan = compiler::plan(&root, &control, &mut |_| {}).unwrap();
    let asset = &plan.textures[0];
    assert_eq!(asset.category, Category::SpriteSheet);
    assert_eq!(asset.options(&profile).algorithm, Algorithm::CatmullRom);
    let expected = textures::process(
        &source,
        profile.scale,
        100,
        &TextureOptions {
            algorithm: Algorithm::CatmullRom,
            ..profile.textures
        },
    )
    .unwrap();
    let preview = compiler::preview(&plan, &profile, asset).unwrap();
    assert_eq!(preview, expected);
    let output = fixture.0.join("mod");
    for expected_hits in [0, 1] {
        assert_eq!(
            build_texture(&plan, &profile, &output, &control, expected_hits),
            preview
        );
    }
    profile.auto_algorithm = false;
    let manual = compiler::preview(&plan, &profile, asset).unwrap();
    assert_ne!(manual, preview);
    assert_eq!(build_texture(&plan, &profile, &output, &control, 0), manual);
}

#[test]
fn inherited_texture_scale_matches_preview_build_and_cache() {
    let fixture = fixture();
    let control = Control::default();
    let mut profile = Profile::default();
    profile.directories.insert(
        "gfx".into(),
        TextureRule {
            scale: Some(Scale::try_from(1.5).unwrap()),
            ..Default::default()
        },
    );
    let plan = compiler::plan(&fixture.0.join("game"), &control, &mut |_| {}).unwrap();
    let output = fixture.0.join("mod");
    let preview = compiler::preview(&plan, &profile, &plan.textures[0]).unwrap();
    assert_eq!(preview.dimensions(), (12, 6));
    for hits in [0, 1] {
        assert_eq!(
            build_texture(&plan, &profile, &output, &control, hits),
            preview
        );
    }
    profile.directories.get_mut("gfx").unwrap().scale = Some(Scale::try_from(2.0).unwrap());
    assert_eq!(
        build_texture(&plan, &profile, &output, &control, 0).dimensions(),
        (16, 8)
    );
}

#[test]
fn streaming_preview_emits_source_first_and_runs_candidates_concurrently() {
    use compiler::{PreviewEvent, PreviewSession};
    use std::sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    };
    use std::time::Duration;

    let fixture = fixture();
    let control = Control::default();
    let plan = compiler::plan(&fixture.0.join("game"), &control, &mut |_| {}).unwrap();
    let algorithms = [Algorithm::Lanczos3, Algorithm::CatmullRom];
    let profile = Profile::default();
    let source_seen = AtomicBool::new(false);
    let results = Mutex::new(Vec::new());
    let (sender, receiver) = mpsc::channel();
    let receiver = Mutex::new(receiver);
    let parallel = rayon::current_num_threads() > 1
        && std::thread::available_parallelism().is_ok_and(|count| count.get() > 1);
    PreviewSession::default()
        .render(
            &plan,
            &profile,
            &plan.textures[0],
            &algorithms,
            &control,
            &|event| match event {
                PreviewEvent::Source { image, .. } => {
                    assert_eq!(image.dimensions(), (8, 4));
                    assert!(!source_seen.swap(true, Ordering::Relaxed));
                }
                PreviewEvent::Candidate { index, image } => {
                    assert!(source_seen.load(Ordering::Relaxed));
                    // Первый обработчик не должен блокировать доставку второго результата.
                    if index == 0 && parallel {
                        receiver
                            .lock()
                            .unwrap()
                            .recv_timeout(Duration::from_secs(5))
                            .unwrap();
                    } else {
                        sender.send(()).unwrap();
                    }
                    results.lock().unwrap().push((index, image.unwrap()));
                }
            },
        )
        .unwrap();
    let mut results = results.into_inner().unwrap();
    results.sort_unstable_by_key(|(index, _)| *index);
    let collected =
        compiler::preview_candidates(&plan, &profile, &plan.textures[0], &algorithms, &control)
            .unwrap();
    for ((index, actual), (name, expected)) in results.iter().zip(&collected[1..]) {
        assert_eq!(
            name,
            &format!("{}: {}", index + 1, algorithms[*index].name())
        );
        assert_eq!(actual, expected);
    }
    assert_eq!(results.len(), algorithms.len());
}

#[test]
fn streaming_preview_cancellation_after_source_stops_candidates() {
    use compiler::{PreviewEvent, PreviewSession};
    let fixture = fixture();
    let control = Control::default();
    let plan = compiler::plan(&fixture.0.join("game"), &control, &mut |_| {}).unwrap();
    let session = PreviewSession::default();
    assert!(
        session
            .render(
                &plan,
                &Profile::default(),
                &plan.textures[0],
                &[Algorithm::Lanczos3, Algorithm::CatmullRom],
                &control,
                &|event| {
                    assert!(matches!(event, PreviewEvent::Source { .. }));
                    control.cancel();
                },
            )
            .is_err()
    );
    // Отмена запроса не делает сохранённую сессию непригодной для следующего превью.
    session
        .render(
            &plan,
            &Profile::default(),
            &plan.textures[0],
            &[],
            &Control::default(),
            &|_| {},
        )
        .unwrap();
}
