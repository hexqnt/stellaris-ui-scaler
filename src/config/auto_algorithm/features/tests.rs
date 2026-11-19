use image::Rgba;

use super::*;

#[test]
fn ignores_hidden_rgb_atlas_boundaries_and_padding() {
    let a = RgbaImage::from_pixel(65, 16, Rgba([0, 0, 0, 0]));
    let b = RgbaImage::from_fn(65, 16, |x, _| {
        Rgba([if x % 2 == 0 { 255 } else { 0 }, 200, 90, 0])
    });
    for source in [&a, &b] {
        let features = ImageFeatures::analyze(source, 2).unwrap();
        assert_eq!(serde_json::to_value(features).unwrap()["color_weight"], 0);
        assert!(features.content_size.is_none());
        assert!(!features.has_flat_edges());
        assert!(!features.limits_sharpening());
        assert!(!matches!(
            features.consensus(),
            Consensus::Agreed(Content::Detailed)
        ));
        assert!(!matches!(
            features.consensus(),
            Consensus::Agreed(Content::Smooth)
        ));
    }
    let atlas = RgbaImage::from_fn(65, 16, |x, _| {
        if x < 32 {
            Rgba([0, 0, 0, 255])
        } else {
            Rgba([255, 255, 255, 255])
        }
    });
    let features = ImageFeatures::analyze(&atlas, 2).unwrap();
    assert_eq!(serde_json::to_value(features).unwrap()["sharp_weight"], 0);
    assert_eq!(serde_json::to_value(features).unwrap()["rough_weight"], 0);
    assert!(!matches!(
        features.consensus(),
        Consensus::Agreed(Content::Smooth)
    ));
    assert_eq!(serde_json::to_value(features).unwrap()["samples"], 64 * 16);
}

#[test]
fn content_bounds_are_per_frame_and_include_sampling_uncertainty() {
    let atlas = RgbaImage::from_fn(128, 64, |x, y| {
        let local_x = x % 64;
        let left = if x < 64 { 4 } else { 44 };
        Rgba([
            80,
            120,
            180,
            if (left..left + 16).contains(&local_x) && (24..40).contains(&y) {
                255
            } else {
                0
            },
        ])
    });
    assert_eq!(
        ImageFeatures::analyze(&atlas, 2).unwrap().content_size,
        Some((16, 16))
    );
    assert_eq!(sampled_span(10, 25, 64, 64), 16);
    assert_eq!(sampled_span(10, 25, 1024, 256), 26);
    assert_eq!(sampled_span(0, 1023, 1024, 256), 1024);
    assert_eq!(sampled_span(10, 10, 1024, 1), 1024);
}

#[test]
fn neutral_frames_require_uniform_sampled_alpha() {
    let uniform = RgbaImage::from_pixel(96, 64, Rgba([20, 30, 40, 128]));
    std::assert_matches!(
        ImageFeatures::analyze(&uniform, 1).unwrap().consensus(),
        Consensus::Neutral
    );
    // Сетка пропускает сам переход альфы между x=1 и x=2, но видит разные значения по его сторонам.
    let stepped = RgbaImage::from_fn(1024, 4, |x, _| {
        Rgba([20, 30, 40, if x < 2 { 0 } else { 255 }])
    });
    let features = ImageFeatures::analyze(&stepped, 1).unwrap();
    assert_eq!(serde_json::to_value(features).unwrap()["alpha_varying"], 0);
    std::assert_matches!(features.consensus(), Consensus::Agreed(Content::Unknown));
}

#[test]
fn sampling_is_bounded_and_layout_is_checked() {
    let source = RgbaImage::new(1024, 1024);
    assert_eq!(
        serde_json::to_value(ImageFeatures::analyze(&source, 1).unwrap()).unwrap()["samples"],
        SAMPLE_BUDGET
    );
    assert_eq!(
        serde_json::to_value(ImageFeatures::analyze(&source, 32).unwrap()).unwrap()["samples"],
        SAMPLE_BUDGET
    );
    assert!(ImageFeatures::analyze(&source, 0).is_err());
    assert!(ImageFeatures::analyze(&source, 1025).is_err());
    assert!(ImageFeatures::analyze(&RgbaImage::new(0, 0), 1).is_err());
}
