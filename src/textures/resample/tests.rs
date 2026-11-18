use image::Rgba;

use super::*;

#[test]
fn color_tables_match_scalar_conversion_for_every_byte() {
    for byte in u8::MIN..=u8::MAX {
        let value = f32::from(byte) / 255.0;
        assert_eq!(SRGB[usize::from(byte)].to_bits(), value.to_bits());
        assert_eq!(
            LINEAR[usize::from(byte)].to_bits(),
            to_linear(value).to_bits()
        );
    }
}

#[test]
fn filtering_matches_scalar_reference_at_block_boundaries() {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .unwrap();
    pool.install(|| {
        for (source_size, target_size) in [
            ((1, 9), (7, 1)),
            ((9, 1), (1, 7)),
            ((17, 11), (63, 19)),
            ((17, 11), (64, 19)),
            ((17, 11), (65, 19)),
            ((17, 11), (129, 5)),
            ((129, 67), (31, 11)),
        ] {
            let atlas = RgbaImage::from_fn(source_size.0 + 5, source_size.1 + 3, |x, y| {
                let bytes = x
                    .wrapping_mul(0x9e37_79b9)
                    .wrapping_add(y.wrapping_mul(0x85eb_ca6b))
                    .to_le_bytes();
                let alpha = match (x + y) % 5 {
                    0 => 0,
                    1 => 1,
                    2 => 254,
                    3 => 255,
                    _ => bytes[3],
                };
                Rgba([bytes[0], bytes[1], bytes[2], alpha])
            });
            let view = atlas.view(3, 2, source_size.0, source_size.1);
            let source = view.to_image();
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
                            resize(&source, target_size.0, target_size.1, &options).unwrap();
                        let expected = scalar_resize(&source, target_size, &options);
                        assert_eq!(
                            result, expected,
                            "{source_size:?} -> {target_size:?}, {options:?}"
                        );
                        let resampler = Resampler::new(source_size, target_size, &options).unwrap();
                        for x in [0, 3, 5] {
                            let view = atlas.view(x, 2, source_size.0, source_size.1);
                            assert_eq!(
                                resampler.resize(view),
                                scalar_resize(&view.to_image(), target_size, &options),
                                "view at {x}, {source_size:?} -> {target_size:?}, {options:?}"
                            );
                        }
                    }
                }
            }
        }
    });
}

// Эталон сохраняет исходный обход по пикселям и скалярное преобразование цветов без таблиц.
fn scalar_resize(source: &RgbaImage, target: (u32, u32), options: &TextureOptions) -> RgbaImage {
    let input: Vec<[f32; 4]> = source
        .pixels()
        .map(|pixel| {
            let alpha = f32::from(pixel[3]) / 255.0;
            let mut value = [0.0; 4];
            for (out, &byte) in value[..3].iter_mut().zip(&pixel.0[..3]) {
                let x = f32::from(byte) / 255.0;
                let x = if options.color_space == ColorSpace::Linear {
                    to_linear(x)
                } else {
                    x
                };
                *out = if options.premultiplied_alpha {
                    x * alpha
                } else {
                    x
                };
            }
            value[3] = alpha;
            value
        })
        .collect();
    let filter = Filter::try_from(options.algorithm).unwrap();
    let horizontal = Weights::new(source.width(), target.0, filter);
    let vertical = Weights::new(source.height(), target.1, filter);
    let mut intermediate = vec![[0.0_f32; 4]; target.0 as usize * source.height() as usize];
    for (row, source) in intermediate
        .chunks_mut(target.0 as usize)
        .zip(input.chunks(source.width() as usize))
    {
        for (out, (start, weights)) in row.iter_mut().zip(horizontal.iter()) {
            for (offset, &weight) in weights.iter().enumerate() {
                for (a, b) in out.iter_mut().zip(source[start + offset]) {
                    *a += b * weight;
                }
            }
        }
    }
    let mut result = RgbaImage::new(target.0, target.1);
    for (row, (start, weights)) in result
        .as_mut()
        .chunks_mut(target.0 as usize * 4)
        .zip(vertical.iter())
    {
        for (x, pixel) in row.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let mut value = [0.0_f32; 4];
            for (offset, &weight) in weights.iter().enumerate() {
                let input = intermediate[(start + offset) * target.0 as usize + x];
                for (a, b) in value.iter_mut().zip(input) {
                    *a += b * weight;
                }
            }
            let alpha = value[3].clamp(0.0, 1.0);
            for (out, mut v) in pixel[..3].iter_mut().zip(value[..3].iter().copied()) {
                if options.premultiplied_alpha {
                    v = if alpha > 1e-6 { v / alpha } else { 0.0 };
                }
                if options.color_space == ColorSpace::Linear {
                    v = from_linear(v.clamp(0.0, 1.0));
                }
                *out = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            }
            pixel[3] = (alpha * 255.0).round() as u8;
        }
    }
    result
}
