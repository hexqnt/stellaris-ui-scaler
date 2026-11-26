use super::*;

#[test]
fn raster_decode_supports_signature_detection_and_tga_fallback() {
    use image::{DynamicImage, ImageFormat, Rgb, RgbImage};
    use std::io::Cursor;

    let source = RgbImage::from_pixel(7, 5, Rgb([73, 121, 199]));
    let source = DynamicImage::ImageRgb8(source);
    for format in [
        ImageFormat::Png,
        ImageFormat::Tga,
        ImageFormat::Bmp,
        ImageFormat::Jpeg,
    ] {
        let mut bytes = Cursor::new(Vec::new());
        source.write_to(&mut bytes, format).unwrap();
        let decoded = textures::decode(bytes.get_ref()).unwrap();
        assert_eq!(decoded.dimensions(), (7, 5), "{format:?}");
        if format == ImageFormat::Jpeg {
            assert!(decoded.pixels().all(|pixel| {
                pixel[3] == 255
                    && pixel.0[..3]
                        .iter()
                        .zip([73, 121, 199])
                        .all(|(&actual, expected)| actual.abs_diff(expected) <= 2)
            }));
        } else {
            assert_eq!(decoded, source.to_rgba8(), "{format:?}");
        }
        assert!(
            textures::decode(&bytes.get_ref()[..8]).is_err(),
            "{format:?}"
        );
    }
}

#[test]
fn dds_dimensions_require_complete_legacy_and_dx10_headers() {
    use image_dds::ddsfile::{AlphaMode, D3D10ResourceDimension, Dds, DxgiFormat, NewDxgiParams};

    let legacy = textures::resource_dds(&image::RgbaImage::new(7, 5)).unwrap();
    let dx10 = Dds::new_dxgi(NewDxgiParams {
        height: 5,
        width: 7,
        depth: None,
        format: DxgiFormat::R8G8B8A8_UNorm,
        mipmap_levels: None,
        array_layers: None,
        caps2: None,
        is_cubemap: false,
        resource_dimension: D3D10ResourceDimension::Texture2D,
        alpha_mode: AlphaMode::Straight,
    })
    .unwrap();
    let mut extended = Vec::new();
    dx10.write(&mut extended).unwrap();
    for (bytes, header_length) in [(&legacy, 128), (&extended, 148)] {
        assert_eq!(textures::dimensions(bytes).unwrap(), (7, 5));
        assert_eq!(
            textures::dimensions(&bytes[..header_length]).unwrap(),
            (7, 5)
        );
        for length in 4..header_length {
            assert!(textures::dimensions(&bytes[..length]).is_err(), "{length}");
        }
    }
}

#[test]
fn frame_filtering_does_not_bleed_and_alpha_does_not_halo() {
    let source = image::RgbaImage::from_fn(8, 4, |x, _| {
        if x < 4 {
            image::Rgba([255, 0, 0, 255])
        } else {
            image::Rgba([0, 0, 255, 255])
        }
    });
    for algorithm in Algorithm::RESAMPLING {
        let options = TextureOptions {
            algorithm,
            sharpening: 0.0,
            ..Default::default()
        };
        let result =
            textures::process(&source, Scale::try_from(1.5).unwrap(), 2, &options).unwrap();
        assert_eq!(result.dimensions(), (12, 6));
        for (x, _, p) in result.enumerate_pixels() {
            if x < 6 {
                assert_eq!(p.0, [255, 0, 0, 255]);
            } else {
                assert_eq!(p.0, [0, 0, 255, 255]);
            }
        }
    }
    let source = image::RgbaImage::from_fn(4, 1, |x, _| {
        if x < 2 {
            image::Rgba([0, 0, 0, 0])
        } else {
            image::Rgba([255, 255, 255, 255])
        }
    });
    let options = TextureOptions {
        algorithm: Algorithm::Bilinear,
        sharpening: 0.0,
        color_space: ColorSpace::Linear,
        ..Default::default()
    };
    let result = textures::process(&source, Scale::try_from(2.0).unwrap(), 1, &options).unwrap();
    for p in result.pixels().filter(|p| p[3] > 0 && p[3] < 255) {
        assert!(p[0] >= 254);
    }
    assert!(textures::process(&source, Scale::default(), 5, &options).is_err());
}

#[test]
fn filters_preserve_constant_color_when_growing_and_shrinking() {
    let color = image::Rgba([73, 121, 199, 128]);
    for (source_size, target_size) in [
        ((1, 1), (7, 11)),
        ((7, 11), (1, 1)),
        ((1, 9), (13, 3)),
        ((9, 1), (3, 13)),
        ((7, 11), (19, 5)),
    ] {
        let source = image::RgbaImage::from_pixel(source_size.0, source_size.1, color);
        for algorithm in Algorithm::RESAMPLING {
            for color_space in [ColorSpace::Srgb, ColorSpace::Linear] {
                for premultiplied_alpha in [false, true] {
                    let options = TextureOptions {
                        algorithm,
                        color_space,
                        premultiplied_alpha,
                        sharpening: 0.0,
                        ..Default::default()
                    };
                    let result =
                        textures::resize(&source, target_size.0, target_size.1, &options).unwrap();
                    assert!(
                        result.pixels().all(|pixel| *pixel == color),
                        "{source_size:?} -> {target_size:?}, {options:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn texture_batches_match_single_worker_and_recover_corrupted_resource_cache() {
    let fixture = fixture();
    let root = fixture.0.join("game");
    let source = image::RgbaImage::from_fn(128, 128, |x, y| {
        image::Rgba([
            u8::try_from(x).unwrap(),
            u8::try_from(y).unwrap(),
            73,
            u8::try_from((x + y) % 256).unwrap(),
        ])
    });
    let mut gfx = String::from("spriteTypes = {");
    for index in 0..17 {
        let path = format!("gfx/batch_{index}.png");
        source.save(root.join(&path)).unwrap();
        write!(
            gfx,
            "spriteType = {{ name = GFX_batch_{index} texturefile = \"{path}\" noOfFrames = 2 }}"
        )
        .unwrap();
    }
    gfx.push('}');
    write(&root, "interface/batches.gfx", &gfx);
    let resource = "gfx/interface/icons/resources/batch.dds";
    let resource_path = root.join(resource);
    fs::create_dir_all(resource_path.parent().unwrap()).unwrap();
    fs::write(resource_path, textures::resource_dds(&source).unwrap()).unwrap();
    let control = Control::default();
    let plan = compiler::plan(&root, &control, &mut |_| {}).unwrap();
    assert_eq!(plan.textures.len(), 19);
    let profile = Profile {
        scale: Scale::try_from(2.0).unwrap(),
        ..Default::default()
    };
    let mut expected = None;
    for threads in [1, 4] {
        let output = fixture.0.join(format!("threads-{threads}/mod"));
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap();
        pool.install(|| {
            let mut progress = Vec::new();
            let report = compiler::build(&plan, &profile, &output, &control, &mut |event| {
                if event.stage == compiler::ProgressStage::Textures {
                    progress.push(event.completed);
                }
            })
            .unwrap();
            assert_eq!(report.cache_hits, 0);
            assert_eq!(progress, (0..plan.textures.len()).collect::<Vec<_>>());
            let manifest = fs::read(output.join(compiler::manifest_name())).unwrap();
            if let Some(expected) = &expected {
                assert_eq!(&manifest, expected);
            } else {
                expected = Some(manifest.clone());
            }
            let scaled =
                textures::decode(&fs::read(output.join(textures::output_name(resource))).unwrap())
                    .unwrap();
            assert_eq!(
                textures::decode(&fs::read(output.join(resource)).unwrap()).unwrap(),
                scaled
            );
            let report = compiler::build(&plan, &profile, &output, &control, &mut |_| {}).unwrap();
            assert_eq!(report.cache_hits, plan.textures.len());
            assert_rebuild_ignores_cache(&plan, &profile, &output, &control);
            let cache = output.parent().unwrap().join(".ui-scaler-cache");
            let resource_bytes = fs::read(output.join(textures::output_name(resource))).unwrap();
            let entry = stellaris_ui_scaler::assets::files(&cache)
                .unwrap()
                .into_iter()
                .find(|path| fs::read(path).unwrap() == resource_bytes)
                .unwrap();
            fs::write(entry, b"incomplete PNG").unwrap();
            let report = compiler::build(&plan, &profile, &output, &control, &mut |_| {}).unwrap();
            assert_eq!(report.cache_hits, plan.textures.len() - 1);
            assert_eq!(
                fs::read(output.join(compiler::manifest_name())).unwrap(),
                manifest
            );
            let cancelled = Control::default();
            let error = compiler::build(&plan, &profile, &output, &cancelled, &mut |event| {
                if event.stage == compiler::ProgressStage::Textures && event.completed == 8 {
                    cancelled.cancel();
                }
            })
            .unwrap_err();
            assert_eq!(error.to_string(), "Operation cancelled");
            assert_eq!(
                fs::read(output.join(compiler::manifest_name())).unwrap(),
                manifest
            );
        });
    }
}

#[test]
fn frame_count_exceeding_texture_width_is_preserved_with_diagnostic() {
    let fixture = fixture();
    let root = fixture.0.join("game");
    write(
        &root,
        "interface/example.gfx",
        "spriteTypes = { spriteType = { name = \"GFX_test\" textureFile = \"gfx/test.png\" noOfFrames = 9 } }",
    );
    let plan = compiler::plan(&root, &Control::default(), &mut |_| {}).unwrap();
    assert!(plan.textures.is_empty());
    assert!(
        plan.diagnostics
            .iter()
            .any(|d| d.rule == "source-frame-layout")
    );
    let report = compiler::build(
        &plan,
        &Profile::default(),
        &fixture.0.join("mod"),
        &Control::default(),
        &mut |_| {},
    )
    .unwrap();
    assert!(report.validation.errors.is_empty());
}

#[test]
fn periodic_texture_has_matching_edges_and_empty_images_are_rejected() {
    let source = image::RgbaImage::from_pixel(4, 4, image::Rgba([100, 150, 200, 255]));
    let output = textures::periodic(
        &source,
        Scale::try_from(1.5).unwrap(),
        1,
        &TextureOptions::default(),
    )
    .unwrap();
    assert_eq!(output.dimensions(), (6, 6));
    assert!(output.pixels().all(|p| p.0 == [100, 150, 200, 255]));
    assert!(
        textures::process(
            &image::RgbaImage::new(0, 0),
            Scale::default(),
            1,
            &TextureOptions::default()
        )
        .is_err()
    );
}

#[test]
fn atlas_processing_rejects_invalid_layouts_and_oversized_outputs() {
    let options = TextureOptions::default();
    let scale = Scale::try_from(8.0).unwrap();
    let source = image::RgbaImage::new(1000, 1001);
    assert!(textures::process(&source, scale, 1, &options).is_err());
    assert!(textures::periodic(&source, scale, 1, &options).is_err());
    for frames in [0, 1001] {
        assert!(textures::process(&source, scale, frames, &options).is_err());
        assert!(textures::periodic(&source, scale, frames, &options).is_err());
    }
    let source = image::RgbaImage::new(6, 2);
    for (width, height) in [(u32::MAX, 1), (3000, 8000), (0, 1), (1, 0)] {
        assert!(textures::compact_background(&source, width, height, &options).is_err());
    }
}

#[test]
fn dds_decode_covers_uncompressed_and_bc1_without_encoder() {
    use image_dds::ddsfile::{D3DFormat, Dds, NewD3dParams};
    for (format, data) in [
        (D3DFormat::A8B8G8R8, [255, 0, 0, 255].repeat(16)),
        (D3DFormat::DXT1, vec![0, 248, 0, 0, 0, 0, 0, 0]),
    ] {
        let mut dds = Dds::new_d3d(NewD3dParams {
            width: 4,
            height: 4,
            depth: None,
            format,
            mipmap_levels: None,
            caps2: None,
        })
        .unwrap();
        dds.data = data;
        let mut bytes = Vec::new();
        dds.write(&mut bytes).unwrap();
        let image = textures::decode(&bytes).unwrap();
        assert_eq!(image.dimensions(), (4, 4));
        assert!(image.pixels().all(|p| p.0 == [255, 0, 0, 255]));
    }
}

#[test]
fn padded_button_atlas_is_scaled_without_widget_scale_fallback() {
    let fixture = fixture();
    let root = fixture.0.join("game");
    write(
        &root,
        "interface/example.gfx",
        r#"spriteTypes = { spriteType = { name = "GFX_test" textureFile = "gfx/test.png" noOfFrames = 3 } }"#,
    );
    write(
        &root,
        "interface/example.gui",
        r#"buttonType = { name = "tab" position = { x = 5 y = 7 } quadTextureSprite = "GFX_test" buttonText = "MANAGEMENT_TAB" buttonFont = "cg_16b" }"#,
    );
    let image = image::RgbaImage::from_fn(7, 2, |x, _| {
        image::Rgba(match x / 2 {
            0 => [255, 0, 0, 255],
            1 => [0, 255, 0, 255],
            2 => [0, 0, 255, 255],
            _ => [255, 0, 255, 255],
        })
    });
    image.save(root.join("gfx/test.png")).unwrap();
    let plan = compiler::plan(&root, &Control::default(), &mut |_| {}).unwrap();
    assert!(plan.diagnostics.iter().any(|d| d.rule == "frame-padding"));
    let profile = Profile {
        scale: Scale::try_from(2.0).unwrap(),
        ..Profile::default()
    };
    let output = fixture.0.join("mod");
    compiler::build(&plan, &profile, &output, &Control::default(), &mut |_| {}).unwrap();
    let atlas = image::open(output.join("gfx/test_scaled.png"))
        .unwrap()
        .to_rgba8();
    assert_eq!(atlas.dimensions(), (12, 4));
    for (x, _, pixel) in atlas.enumerate_pixels() {
        assert_eq!(pixel, image.get_pixel((x / 4) * 2, 0));
    }
    let gui = fs::read_to_string(output.join("interface/example.gui")).unwrap();
    assert!(gui.contains("position = { x = 10 y = 14 }"));
    assert!(gui.contains("buttonText = \"MANAGEMENT_TAB\""));
    assert!(!gui.contains("scale ="));
    let gfx = fs::read_to_string(output.join("interface/example.gfx")).unwrap();
    assert!(gfx.contains("noOfFrames = 3"));
    assert!(gfx.contains("test_scaled.png"));
}

#[test]
fn resource_icons_are_overridden_at_engine_dds_paths() {
    let fixture = fixture();
    let root = fixture.0.join("game");
    let path = "gfx/interface/icons/resources/energy.dds";
    let grey = "gfx/interface/icons/resources/energy_grey.dds";
    let image = image::RgbaImage::from_pixel(18, 18, image::Rgba([30, 70, 120, 160]));
    let bytes = textures::resource_dds(&image).unwrap();
    fs::create_dir_all(root.join("gfx/interface/icons/resources")).unwrap();
    fs::write(root.join(path), &bytes).unwrap();
    fs::write(root.join(grey), &bytes).unwrap();
    write(
        &root,
        "interface/resources.gfx",
        r#"spriteTypes = { spriteType = { name = "GFX_resource_energy" textureFile = "gfx/interface/icons/resources/energy.dds" } }"#,
    );
    let plan = compiler::plan(&root, &Control::default(), &mut |_| {}).unwrap();
    let profile = Profile {
        scale: Scale::try_from(2.0).unwrap(),
        ..Profile::default()
    };
    let output = fixture.0.join("mod");
    compiler::build(&plan, &profile, &output, &Control::default(), &mut |_| {}).unwrap();
    for path in [path, grey] {
        let dds = fs::read(output.join(path)).unwrap();
        assert!(dds.starts_with(b"DDS "));
        // Проверяем контракт загрузчика игры отдельно от обратного декодирования нашей библиотекой.
        let header = image_dds::ddsfile::Dds::read(&mut std::io::Cursor::new(&dds)).unwrap();
        assert_eq!(
            header.get_d3d_format(),
            Some(image_dds::ddsfile::D3DFormat::A8R8G8B8)
        );
        assert_eq!(&header.data[..4], &[120, 70, 30, 160]);
        let decoded = textures::decode(&dds).unwrap();
        let png =
            textures::decode(&fs::read(output.join(textures::output_name(path))).unwrap()).unwrap();
        assert_eq!(decoded.dimensions(), (36, 36));
        assert_eq!(decoded, png);
    }
    let manifest: compiler::Manifest =
        serde_json::from_slice(&fs::read(output.join(compiler::manifest_name())).unwrap()).unwrap();
    assert!(manifest.files.contains_key(path));
    assert!(manifest.files.contains_key(grey));
}

fn assert_rebuild_ignores_cache(
    plan: &compiler::BuildPlan,
    profile: &Profile,
    output: &std::path::Path,
    control: &Control,
) {
    let manifest = fs::read(output.join(compiler::manifest_name())).unwrap();
    let rebuilt = compiler::build_with_cache(
        plan,
        profile,
        output,
        compiler::CacheMode::Rebuild,
        control,
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(rebuilt.cache_hits, 0);
    assert_eq!(
        fs::read(output.join(compiler::manifest_name())).unwrap(),
        manifest
    );
    let reused = compiler::build(plan, profile, output, control, &mut |_| {}).unwrap();
    assert_eq!(reused.cache_hits, plan.textures.len());
}
