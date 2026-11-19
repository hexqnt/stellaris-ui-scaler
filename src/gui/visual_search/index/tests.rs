use std::collections::BTreeMap;

use image::Rgba;

use super::*;
use crate::{
    assets::{Installation, Source},
    compiler::TextureAsset,
    config::Category,
    fonts,
    test_support::Temp,
};

fn icon(horizontal: bool) -> RgbaImage {
    RgbaImage::from_fn(32, 32, |x, y| {
        if if horizontal {
            y > 12 && y < 20
        } else {
            x > 12 && x < 20
        } {
            Rgba([200, 170, 50, 255])
        } else {
            Rgba([20, 30, 40, 255])
        }
    })
}

fn plan(temp: &Temp, images: &[(&RgbaImage, u32)]) -> BuildPlan {
    let mut resources = BTreeMap::new();
    let mut textures = Vec::new();
    for (i, (image, frames)) in images.iter().enumerate() {
        let path = format!("gfx/{i}.png");
        let file = temp.0.join(format!("{i}.png"));
        image.save(&file).unwrap();
        resources.insert(path.clone(), Source::File(file));
        textures.push(TextureAsset {
            path,
            width: image.width(),
            height: image.height(),
            frames: *frames,
            category: Category::Icon,
            tiled: false,
        });
    }
    BuildPlan {
        game: Installation {
            root: temp.0.clone(),
            source_mods: vec![],
            version: String::new(),
            resources,
            archives: 0,
        },
        textures,
        fonts: fonts::Catalog::default(),
        sources: BTreeMap::new(),
        sprites: vec![],
        diagnostics: vec![],
    }
}

#[test]
fn scaled_and_recolored_query_ranks_shape_above_distractor() {
    let temp = Temp::new();
    let wanted = icon(true);
    let distractor = icon(false);
    let plan = plan(&temp, &[(&distractor, 1), (&wanted, 1)]);
    let mut query = imageops::resize(&wanted, 71, 71, imageops::FilterType::CatmullRom);
    for p in query.pixels_mut() {
        p[0] /= 2;
        p[2] = p[2].saturating_add(50);
    }
    let control = Control::default();
    for threads in [1, 3] {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap()
            .install(|| {
                let index = Index::build(&plan, &control, &AtomicUsize::new(0)).unwrap();
                let hits = index
                    .search(&Query::new(&query).unwrap(), &control, |_| {
                        Scale::try_from(1.0).unwrap()
                    })
                    .unwrap();
                assert_eq!(hits[0].asset, 1);
                assert_eq!(hits.len(), 2);
            });
    }
}

#[test]
fn frames_and_transparent_margins_are_searchable_and_assets_are_unique() {
    let temp = Temp::new();
    let wanted = icon(true);
    let mut sheet = RgbaImage::new(96, 48);
    imageops::replace(&mut sheet, &icon(false), 8, 8);
    imageops::replace(&mut sheet, &wanted, 56, 8);
    let plan = plan(&temp, &[(&icon(false), 1), (&sheet, 2)]);
    let control = Control::default();
    let index = Index::build(&plan, &control, &AtomicUsize::new(0)).unwrap();
    let hits = index
        .search(&Query::new(&wanted).unwrap(), &control, |_| {
            Scale::try_from(1.0).unwrap()
        })
        .unwrap();
    assert_eq!(hits.len(), 2);
    assert_eq!((hits[0].asset, hits[0].frame), (1, 1));
    assert_eq!(
        hits[0].region,
        Region {
            x: 56,
            y: 8,
            width: 32,
            height: 32
        }
    );
}

#[test]
fn unreadable_resources_are_skipped_and_cancellation_propagates() {
    let temp = Temp::new();
    let mut plan = plan(&temp, &[(&icon(true), 1), (&icon(false), 1)]);
    plan.textures[0].path = "gfx/missing.dds".into();
    let completed = AtomicUsize::new(0);
    let control = Control::default();
    let index = Index::build(&plan, &control, &completed).unwrap();
    assert_eq!(index.skipped, 1);
    assert_eq!(completed.load(Ordering::Relaxed), 2);
    assert!(Query::new(&RgbaImage::new(1, 1)).is_err());
    control.cancel();
    assert!(Index::build(&plan, &control, &completed).is_err());
    assert!(
        index
            .search(&Query::new(&icon(false)).unwrap(), &control, |_| {
                Scale::try_from(1.0).unwrap()
            })
            .is_err()
    );
}

#[test]
fn transparent_silhouettes_match_composited_screenshots() {
    let temp = Temp::new();
    let silhouette = |horizontal| {
        let mut image = icon(horizontal);
        for pixel in image.pixels_mut() {
            if pixel[0] == 20 {
                *pixel = Rgba([0, 0, 0, 0]);
            }
        }
        image
    };
    let wanted = silhouette(true);
    let distractor = silhouette(false);
    let plan = plan(&temp, &[(&distractor, 1), (&wanted, 1)]);
    let mut screenshot = RgbaImage::from_pixel(32, 32, Rgba([45, 65, 85, 255]));
    imageops::overlay(&mut screenshot, &wanted, 0, 0);
    let control = Control::default();
    let index = Index::build(&plan, &control, &AtomicUsize::new(0)).unwrap();
    let hits = index
        .search(&Query::new(&screenshot).unwrap(), &control, |_| {
            Scale::try_from(1.0).unwrap()
        })
        .unwrap();
    assert_eq!(hits[0].asset, 1);
}

#[test]
fn empty_index_still_observes_cancellation() {
    let index = Index {
        entries: Vec::new(),
        skipped: 0,
    };
    let control = Control::default();
    let query = Query::new(&icon(true)).unwrap();
    assert!(
        index
            .search(&query, &control, |_| Scale::try_from(1.0).unwrap())
            .unwrap()
            .is_empty()
    );
    control.cancel();
    assert!(
        index
            .search(&query, &control, |_| Scale::try_from(1.0).unwrap())
            .is_err()
    );
}

#[test]
fn equal_scores_have_the_same_order_in_different_thread_pools() {
    let temp = Temp::new();
    let image = icon(true);
    let plan = plan(&temp, &[(&image, 1), (&image, 1), (&image, 1)]);
    let query = Query::new(&image).unwrap();
    let control = Control::default();
    let index = Index::build(&plan, &control, &AtomicUsize::new(0)).unwrap();
    for threads in [1, 3] {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap()
            .install(|| {
                let hits = index
                    .search(&query, &control, |_| Scale::try_from(1.0).unwrap())
                    .unwrap();
                assert_eq!(
                    hits.iter().map(|hit| hit.asset).collect::<Vec<_>>(),
                    [0, 1, 2]
                );
            });
    }
}

#[test]
fn nearby_dimensions_beat_many_smaller_lookalikes() {
    let temp = Temp::new();
    let wanted = icon(true);
    let small = imageops::resize(&wanted, 8, 8, imageops::FilterType::Triangle);
    let mut images = vec![(&small, 1); 65];
    images.push((&wanted, 1));
    let plan = plan(&temp, &images);
    let mut query = wanted.clone();
    imageops::replace(&mut query, &wanted, 1, 0);
    let control = Control::default();
    let index = Index::build(&plan, &control, &AtomicUsize::new(0)).unwrap();
    let hits = index
        .search(&Query::new(&query).unwrap(), &control, |_| {
            Scale::try_from(1.0).unwrap()
        })
        .unwrap();
    assert_eq!(hits[0].asset, 65);
}

#[test]
fn grayscale_brightness_is_part_of_color_similarity() {
    let temp = Temp::new();
    let dark = RgbaImage::from_pixel(32, 32, Rgba([25, 25, 25, 255]));
    let light = RgbaImage::from_pixel(32, 32, Rgba([230, 230, 230, 255]));
    let wanted = RgbaImage::from_pixel(32, 32, Rgba([110, 110, 110, 255]));
    let plan = plan(&temp, &[(&dark, 1), (&light, 1), (&wanted, 1)]);
    let query = RgbaImage::from_pixel(32, 32, Rgba([115, 115, 115, 255]));
    let control = Control::default();
    let index = Index::build(&plan, &control, &AtomicUsize::new(0)).unwrap();
    let hits = index
        .search(&Query::new(&query).unwrap(), &control, |_| {
            Scale::try_from(1.0).unwrap()
        })
        .unwrap();
    assert_eq!(hits[0].asset, 2);
}

#[test]
fn matching_palette_beats_matching_shape_with_wrong_colors() {
    let temp = Temp::new();
    let wrong_colors = icon(true);
    let recolor = |mut image: RgbaImage| {
        for pixel in image.pixels_mut() {
            if pixel[0] == 200 {
                *pixel = Rgba([40, 140, 240, 255]);
            }
        }
        image
    };
    let matching_palette = recolor(icon(false));
    let query = recolor(icon(true));
    let plan = plan(&temp, &[(&wrong_colors, 1), (&matching_palette, 1)]);
    let control = Control::default();
    let index = Index::build(&plan, &control, &AtomicUsize::new(0)).unwrap();
    let hits = index
        .search(&Query::new(&query).unwrap(), &control, |_| {
            Scale::try_from(1.0).unwrap()
        })
        .unwrap();
    assert_eq!(hits[0].asset, 1);
}

#[test]
fn rendered_frame_dimensions_use_the_asset_scale() {
    let temp = Temp::new();
    let mut sheet = RgbaImage::new(96, 32);
    imageops::replace(&mut sheet, &icon(true), 0, 0);
    imageops::replace(&mut sheet, &icon(false), 32, 0);
    imageops::replace(&mut sheet, &icon(false), 64, 0);
    let distractor = imageops::resize(&icon(true), 48, 48, imageops::FilterType::Triangle);
    let query = imageops::resize(&icon(true), 64, 64, imageops::FilterType::Triangle);
    let plan = plan(&temp, &[(&distractor, 1), (&sheet, 3)]);
    let control = Control::default();
    let index = Index::build(&plan, &control, &AtomicUsize::new(0)).unwrap();
    let hits = index
        .search(&Query::new(&query).unwrap(), &control, |asset| {
            Scale::try_from(if asset == 1 { 2.0 } else { 1.0 }).unwrap()
        })
        .unwrap();
    assert_eq!((hits[0].asset, hits[0].frame), (1, 0));
    let raw_hits = index
        .search(&Query::new(&query).unwrap(), &control, |_| {
            Scale::try_from(1.0).unwrap()
        })
        .unwrap();
    assert_eq!(raw_hits[0].asset, 0);
}
