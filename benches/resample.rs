use std::{
    hint::black_box,
    time::{Duration, Instant},
};

use image::{Rgba, RgbaImage};
use stellaris_ui_scaler::{
    config::{Algorithm, ColorSpace, Scale, TextureOptions},
    textures,
};

fn main() {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .unwrap();
    pool.install(|| {
        for (name, source_size, target_size, frames) in [
            ("icon", (32, 32), (64, 64), 1),
            ("background", (512, 512), (1024, 1024), 1),
            ("downscale", (768, 512), (192, 128), 1),
            ("atlas", (2048, 32), (4096, 64), 64),
        ] {
            let source = RgbaImage::from_fn(source_size.0, source_size.1, |x, y| {
                let bytes = x
                    .wrapping_mul(73)
                    .wrapping_add(y.wrapping_mul(151))
                    .to_le_bytes();
                Rgba([
                    bytes[0],
                    bytes[1],
                    bytes[0].wrapping_add(bytes[1]),
                    bytes[0].wrapping_mul(13),
                ])
            });
            for algorithm in [
                Algorithm::Bilinear,
                Algorithm::CatmullRom,
                Algorithm::Lanczos3,
            ] {
                for color_space in [ColorSpace::Srgb, ColorSpace::Linear] {
                    let options = TextureOptions {
                        algorithm,
                        color_space,
                        sharpening: 0.0,
                        ..Default::default()
                    };
                    let render = || {
                        if frames == 1 {
                            textures::resize(
                                black_box(&source),
                                target_size.0,
                                target_size.1,
                                black_box(&options),
                            )
                        } else {
                            textures::process(
                                black_box(&source),
                                Scale::try_from(2.0).unwrap(),
                                frames,
                                black_box(&options),
                            )
                        }
                        .unwrap()
                    };
                    let expected = render();
                    let start = Instant::now();
                    let mut iterations = 0_u32;
                    while start.elapsed() < Duration::from_millis(250) || iterations < 3 {
                        black_box(render());
                        iterations += 1;
                    }
                    println!(
                        "{name}/{algorithm:?}/{color_space:?}: {:.1} us, {}",
                        start.elapsed().as_secs_f64() * 1e6 / f64::from(iterations),
                        blake3::hash(expected.as_raw())
                    );
                }
            }
        }
    });
}
