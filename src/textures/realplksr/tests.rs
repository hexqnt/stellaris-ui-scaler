use image::Rgba;

use super::*;

#[test]
fn tile_cores_cover_each_axis_once_and_context_stays_inside_the_frame() {
    for length in [1, 17, CORE - 1, CORE, CORE + 1, CORE * 2 + 13] {
        let mut covered = Vec::new();
        for axis in TileAxis::tiles(length, Boundary::Image) {
            assert!(axis.input_len <= CORE + 2 * HALO);
            assert!(axis.input_start >= 0);
            assert!(axis.input_start + i64::from(axis.input_len) <= i64::from(length));
            covered.extend(axis.start..axis.start + axis.core_len);
            for offset in 0..axis.core_len {
                assert_eq!(
                    axis.sample((axis.crop + offset) as usize, length),
                    axis.start + offset
                );
            }
        }
        assert_eq!(covered, (0..length).collect::<Vec<_>>());
        let periodic = TileAxis::tiles(length, Boundary::Periodic).next().unwrap();
        assert_eq!(periodic.crop, HALO);
        for offset in 0..periodic.input_len {
            assert_eq!(
                periodic.sample(offset as usize, length),
                (i64::from(offset) - i64::from(HALO)).rem_euclid(i64::from(length)) as u32
            );
        }
    }
}

#[test]
fn transparent_rgb_is_extended_without_reading_outside_a_sprite_frame() {
    let atlas = RgbaImage::from_fn(11, 7, |x, y| {
        if x == 5 && y == 3 {
            Rgba([90, 120, 200, 128])
        } else if !(3..=7).contains(&x) {
            Rgba([255, 0, 0, 255])
        } else {
            Rgba([0, 255, 0, 0])
        }
    });
    let source = atlas.view(3, 1, 5, 5);
    let filled = extend_transparent(source);
    assert!(filled.pixels().all(|pixel| pixel.0[..3] == [90, 120, 200]));
    assert_eq!(atlas.get_pixel(3, 1).0, [0, 255, 0, 0]);
    assert_eq!(filled.get_pixel(2, 2)[3], 128);
}

#[test]
fn missing_or_modified_models_fail_before_loading_the_runtime() {
    let directory = crate::test_support::Temp::new();
    assert!(
        Model::load_from(&directory.0, NonZeroUsize::MIN)
            .err()
            .unwrap()
            .to_string()
            .contains("Cannot read")
    );
    fs::write(directory.0.join("realplksr-2x.onnx"), b"not the model").unwrap();
    assert!(
        Model::load_from(&directory.0, NonZeroUsize::MIN)
            .err()
            .unwrap()
            .to_string()
            .contains("checksum mismatch")
    );
}

#[test]
#[ignore = "Requires the resources prepared by tools/prepare_ai.py"]
fn realplksr_inference_matches_the_reference_rgb_and_can_cancel_between_tiles() {
    let source = RgbaImage::from_fn(17, 13, |x, y| {
        Rgba([
            ((x * 17 + y * 3) % 256) as u8,
            ((x * 5 + y * 19) % 256) as u8,
            ((x * 11 + y * 7) % 256) as u8,
            255,
        ])
    });
    let mut model = Model::load(NonZeroUsize::MIN).unwrap();
    let output = model
        .upscale(source.view(0, 0, 17, 13), Boundary::Image, &|| Ok(()))
        .unwrap();
    assert_eq!(output.dimensions(), (34, 26));
    // Эталон получен через Python ONNX Runtime с теми же весами, RGB/NCHW и float32.
    for ((x, y), expected) in [
        ((0, 0), [0, 0, 0]),
        ((33, 0), [15, 79, 170]),
        ((0, 25), [39, 219, 83]),
        ((33, 25), [67, 71, 0]),
        ((16, 12), [151, 151, 127]),
        ((9, 8), [82, 96, 73]),
    ] {
        assert!(
            output.get_pixel(x, y).0[..3]
                .iter()
                .zip(expected)
                .all(|(&a, b)| a.abs_diff(b) <= 1)
        );
    }
    let source = RgbaImage::from_pixel(CORE + 1, 9, Rgba([90, 120, 200, 255]));
    let calls = std::cell::Cell::new(0);
    let error = model
        .upscale(
            source.view(0, 0, source.width(), source.height()),
            Boundary::Image,
            &|| {
                calls.set(calls.get() + 1);
                ensure!(calls.get() < 2, "Operation cancelled");
                Ok(())
            },
        )
        .unwrap_err();
    assert_eq!(error.to_string(), "Operation cancelled");
    assert_eq!(calls.get(), 2);
}
