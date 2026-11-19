use super::*;

fn select(
    path: &str,
    category: Category,
    size: Option<(u32, u32)>,
    scale: Scale,
    features: Option<&ImageFeatures>,
) -> Option<AlgorithmRecommendation> {
    Selection::new(path, category, size, scale).resolve(features)
}

fn recommend(
    path: &str,
    category: Category,
    size: Option<(u32, u32)>,
) -> Option<AlgorithmRecommendation> {
    Purpose::new(path, category).recommend(size)
}

#[test]
fn conflicting_frame_structure_uses_the_same_fallback_in_any_order() {
    for order in [[0, 1, 2], [2, 0, 1], [1, 2, 0]] {
        let atlas = crate::test_images::mixed_atlas(order);
        let features = ImageFeatures::analyze(&atlas, 3).unwrap();
        std::assert_matches!(features.consensus(), Consensus::Mixed);
        let choice = select(
            "gfx/atlas.dds",
            Category::SpriteSheet,
            Some((64, 64)),
            Scale::try_from(2.0).unwrap(),
            Some(&features),
        )
        .unwrap();
        assert_eq!(choice.algorithm, Algorithm::CatmullRom);
        assert_eq!(
            choice.reason,
            "mixed sprite-frame structure; texture purpose"
        );
        assert!(features.limits_sharpening());
    }
}

#[test]
fn empty_and_uniform_frames_do_not_hide_detailed_states() {
    use image::{Rgba, RgbaImage};
    let atlas = RgbaImage::from_fn(256, 64, |x, y| match x / 64 {
        1 => Rgba([255, 0, 255, 0]),
        2 => Rgba([128, 128, 128, 255]),
        _ => {
            let value = if (x + y) % 2 == 0 { 32 } else { 224 };
            Rgba([value, value, value, 255])
        }
    });
    let features = ImageFeatures::analyze(&atlas, 4).unwrap();
    std::assert_matches!(features.consensus(), Consensus::Agreed(Content::Detailed));
    assert_eq!(
        select(
            "gfx/atlas.dds",
            Category::SpriteSheet,
            Some((64, 64)),
            Scale::try_from(2.0).unwrap(),
            Some(&features)
        )
        .unwrap()
        .algorithm,
        Algorithm::Lanczos3
    );
}

#[test]
fn low_contrast_random_variation_is_not_confused_with_a_periodic_pattern() {
    use image::{Rgba, RgbaImage};
    let noise = crate::test_images::unstructured();
    let features = ImageFeatures::analyze(&noise, 1).unwrap();
    assert!(
        matches!(
            features.consensus(),
            Consensus::Agreed(Content::Unstructured)
        ),
        "{features:?}"
    );
    let choice = select(
        "gfx/icon.dds",
        Category::Icon,
        Some(noise.dimensions()),
        Scale::try_from(2.0).unwrap(),
        Some(&features),
    )
    .unwrap();
    assert_eq!(choice.algorithm, Algorithm::CatmullRom);
    assert!(features.limits_sharpening());

    let pattern = RgbaImage::from_fn(96, 64, |x, y| {
        let value = if (x + y) % 2 == 0 { 112 } else { 144 };
        Rgba([value, value, value, 255])
    });
    let features = ImageFeatures::analyze(&pattern, 1).unwrap();
    std::assert_matches!(features.consensus(), Consensus::Agreed(Content::Detailed));
    assert_eq!(
        select(
            "gfx/icon.dds",
            Category::Icon,
            Some(pattern.dimensions()),
            Scale::try_from(2.0).unwrap(),
            Some(&features)
        )
        .unwrap()
        .algorithm,
        Algorithm::Lanczos3
    );
    assert!(!features.limits_sharpening());
}

#[test]
fn illustrated_texture_is_not_mistaken_for_flat_ui() {
    use image::{Rgba, RgbaImage};
    for textured in [false, true] {
        let source = RgbaImage::from_fn(64, 64, |x, y| {
            let base = if (x / 16) % 2 == 0 { 32 } else { 192 };
            let texture = if textured && y < 16 && (x + y) % 2 == 0 {
                16
            } else {
                0
            };
            Rgba([base + texture, base + texture, base + texture, 255])
        });
        let features = ImageFeatures::analyze(&source, 1).unwrap();
        assert!(features.has_flat_edges());
        let choice = select(
            "gfx/event_pictures/test.dds",
            Category::Background,
            Some(source.dimensions()),
            Scale::try_from(2.0).unwrap(),
            Some(&features),
        )
        .unwrap();
        assert_eq!(
            choice.algorithm,
            if textured {
                Algorithm::Lanczos3
            } else {
                Algorithm::Mitchell
            }
        );
    }
}

#[test]
fn transparent_padding_does_not_turn_a_small_detailed_icon_into_large_artwork() {
    use image::{Rgba, RgbaImage};
    for (canvas, size, expected) in [
        (16, 16, Algorithm::CatmullRom),
        (64, 16, Algorithm::CatmullRom),
        (256, 16, Algorithm::CatmullRom),
        (512, 16, Algorithm::CatmullRom),
        (64, 40, Algorithm::Lanczos3),
    ] {
        let left = (canvas - size) / 2;
        let source = RgbaImage::from_fn(canvas, canvas, |x, y| {
            if (left..left + size).contains(&x) && (left..left + size).contains(&y) {
                let value = if (x + y) % 2 == 0 { 32 } else { 224 };
                Rgba([value, value, value, 255])
            } else {
                Rgba([255, 0, 255, 0])
            }
        });
        let features = ImageFeatures::analyze(&source, 1).unwrap();
        let choice = select(
            "gfx/unknown.dds",
            Category::Icon,
            Some(source.dimensions()),
            Scale::try_from(2.0).unwrap(),
            Some(&features),
        )
        .unwrap();
        assert_eq!(
            choice.algorithm, expected,
            "canvas={canvas}, content={size}, {features:?}"
        );
    }
}

#[test]
fn pixels_distinguish_flat_edges_gradients_and_fine_detail() {
    use image::{Rgba, RgbaImage};
    let flat = crate::test_images::flat_graphic();
    let color_gradient = RgbaImage::from_fn(64, 64, |x, _| {
        Rgba([u8::try_from(x * 4).unwrap(), 40, 60, 255])
    });
    let alpha_gradient = RgbaImage::from_fn(64, 64, |x, _| {
        Rgba([100, 120, 140, u8::try_from(x * 4).unwrap()])
    });
    let detailed = RgbaImage::from_fn(64, 64, |x, y| {
        let value = if (x + y) % 2 == 0 { 32 } else { 224 };
        Rgba([value, value, value, 255])
    });
    let silhouette = RgbaImage::from_fn(64, 64, |x, y| {
        Rgba([
            80,
            180,
            120,
            if (8..56).contains(&x) && (8..56).contains(&y) {
                255
            } else {
                0
            },
        ])
    });
    for (source, expected) in [
        (flat, Algorithm::Mitchell),
        (color_gradient, Algorithm::Bilinear),
        (alpha_gradient, Algorithm::Bilinear),
        (detailed, Algorithm::Lanczos3),
        (silhouette, Algorithm::Mitchell),
    ] {
        let features = ImageFeatures::analyze(&source, 1).unwrap();
        let choice = select(
            "gfx/unknown.dds",
            Category::Icon,
            Some(source.dimensions()),
            Scale::try_from(2.0).unwrap(),
            Some(&features),
        )
        .unwrap();
        assert_eq!(choice.algorithm, expected, "{features:?}");
    }
}

#[test]
fn scale_uses_rounded_frame_dimensions_and_masks_keep_their_role() {
    for (size, scale, expected) in [
        ((64, 64), 0.5, Algorithm::Lanczos3),
        ((4, 4), 1.01, Algorithm::Nearest),
        ((4, 4), 1.5, Algorithm::CatmullRom),
    ] {
        let choice = select(
            "gfx/unknown.dds",
            Category::Icon,
            Some(size),
            Scale::try_from(scale).unwrap(),
            None,
        )
        .unwrap();
        assert_eq!(choice.algorithm, expected);
    }
    assert_eq!(
        select(
            "gfx/unknown.dds",
            Category::Mask,
            Some((64, 64)),
            Scale::try_from(0.5).unwrap(),
            None
        )
        .unwrap()
        .algorithm,
        Algorithm::Bilinear
    );
}

#[test]
fn purpose_precedes_size_and_generic_directories() {
    for (path, category, size, expected) in [
        (
            "gfx/icons/energy_mask.dds",
            Category::Icon,
            None,
            Algorithm::Bilinear,
        ),
        (
            "gfx/gradients/large.dds",
            Category::Background,
            None,
            Algorithm::Bilinear,
        ),
        (
            "gfx/icons/maskot.dds",
            Category::Icon,
            None,
            Algorithm::CatmullRom,
        ),
        (
            "gfx/portraits/robot.dds",
            Category::Icon,
            None,
            Algorithm::Lanczos3,
        ),
        (
            "gfx/icons/relics/relic.dds",
            Category::Icon,
            None,
            Algorithm::Lanczos3,
        ),
        (
            "gfx/portraits/portrait_frame.dds",
            Category::Background,
            None,
            Algorithm::CatmullRom,
        ),
        (
            "gfx/animation.dds",
            Category::SpriteSheet,
            Some((32, 32)),
            Algorithm::CatmullRom,
        ),
        (
            "gfx/animation.dds",
            Category::SpriteSheet,
            Some((257, 32)),
            Algorithm::Lanczos3,
        ),
        (
            "gfx/portraits/animation.dds",
            Category::SpriteSheet,
            Some((32, 32)),
            Algorithm::Lanczos3,
        ),
        (
            "gfx/BUTTONS/large.dds",
            Category::Background,
            None,
            Algorithm::CatmullRom,
        ),
        (
            "gfx/background.dds",
            Category::Background,
            None,
            Algorithm::Lanczos3,
        ),
    ] {
        assert_eq!(
            recommend(path, category, size).unwrap().algorithm,
            expected,
            "{path}"
        );
    }
    assert!(recommend("gfx/unknown.dds", Category::SpriteSheet, None).is_none());
}
