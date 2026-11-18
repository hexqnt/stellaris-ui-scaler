// Размеры ограничены лимитом изображения; преобразования каналов насыщаются явно.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
#![allow(
    clippy::suboptimal_flops,
    reason = "Сохраняем порядок операций эталонных формул фильтров"
)]

use std::{f32::consts::PI, ops::Range, simd::f32x4, sync::LazyLock};

use anyhow::{Result, ensure};
use image::{GenericImageView, RgbaImage, SubImage, imageops};
use rayon::prelude::*;

use crate::config::{Algorithm, ColorSpace, TextureOptions};

use super::check_dimensions;

const SRGB: [f32; 256] = {
    let mut values = [0.0; 256];
    let mut i = 0;
    while i < values.len() {
        values[i] = i as f32 / 255.0;
        i += 1;
    }
    values
};

// Таблица точна для всех входных u8: powf вычисляется лишь при первом использовании линейного пространства.
static LINEAR: LazyLock<[f32; 256]> = LazyLock::new(|| SRGB.map(to_linear));

// Отсчёты одного окна последовательны; индекс нужен только для начала окна.
struct Window {
    source_start: usize,
    weights: Range<usize>,
}

struct Weights {
    windows: Vec<Window>,
    values: Vec<f32>,
}

#[derive(Clone, Copy, PartialEq)]
enum Filter {
    Nearest,
    Bilinear,
    CatmullRom,
    Mitchell,
    Lanczos2,
    Lanczos3,
}

impl TryFrom<Algorithm> for Filter {
    type Error = anyhow::Error;

    fn try_from(algorithm: Algorithm) -> Result<Self> {
        Ok(match algorithm {
            Algorithm::Nearest => Self::Nearest,
            Algorithm::Bilinear => Self::Bilinear,
            Algorithm::CatmullRom => Self::CatmullRom,
            Algorithm::Mitchell => Self::Mitchell,
            Algorithm::Lanczos2 => Self::Lanczos2,
            Algorithm::Lanczos3 => Self::Lanczos3,
            Algorithm::RealPlksr2x => {
                anyhow::bail!("RealPLKSR requires the neural texture processor")
            }
        })
    }
}

impl Weights {
    fn new(source: u32, target: u32, algorithm: Filter) -> Self {
        let ratio = source as f32 / target as f32;
        let filter_scale = ratio.max(1.0);
        let radius = match algorithm {
            Filter::Nearest => 0.5,
            Filter::Bilinear => 1.0,
            Filter::Lanczos3 => 3.0,
            _ => 2.0,
        } * filter_scale;
        let mut weights = Self {
            windows: Vec::with_capacity(target as usize),
            values: Vec::new(),
        };
        for i in 0..target {
            let start = weights.values.len();
            let center = (i as f32 + 0.5) * ratio - 0.5;
            let source_start = if algorithm == Filter::Nearest {
                weights.values.push(1.0);
                center.round().clamp(0.0, (source - 1) as f32) as usize
            } else {
                let left = (center - radius).ceil().max(0.0) as u32;
                let right = (center + radius).floor().min((source - 1) as f32) as u32;
                weights.values.extend(
                    (left..=right).map(|n| kernel((n as f32 - center) / filter_scale, algorithm)),
                );
                let taps = &mut weights.values[start..];
                let total: f32 = taps.iter().sum();
                for value in taps {
                    *value /= total;
                }
                left as usize
            };
            weights.windows.push(Window {
                source_start,
                weights: start..weights.values.len(),
            });
        }
        weights
    }

    fn window(&self, window: &Window) -> (usize, &[f32]) {
        (window.source_start, &self.values[window.weights.clone()])
    }

    fn iter(&self) -> impl Iterator<Item = (usize, &[f32])> {
        self.windows.iter().map(|window| self.window(window))
    }

    fn par_iter(&self) -> impl IndexedParallelIterator<Item = (usize, &[f32])> {
        self.windows.par_iter().map(|window| self.window(window))
    }
}

/// # Errors
/// Возвращает ошибку недопустимых размеров изображения.
pub(super) fn resize(
    source: &RgbaImage,
    width: u32,
    height: u32,
    options: &TextureOptions,
) -> Result<RgbaImage> {
    let resampler = Resampler::new(source.dimensions(), (width, height), options)?;
    Ok(resampler.resize(source.view(0, 0, source.width(), source.height())))
}

/// Один набор весов для всех одинаковых кадров; исходные строки читаются без копии RGBA.
pub(super) struct Resampler<'a> {
    source_size: (u32, u32),
    target_size: (u32, u32),
    horizontal: Weights,
    vertical: Weights,
    options: &'a TextureOptions,
}

impl<'a> Resampler<'a> {
    pub(super) fn new(
        source_size: (u32, u32),
        target_size: (u32, u32),
        options: &'a TextureOptions,
    ) -> Result<Self> {
        let (sw, sh) = source_size;
        let (width, height) = target_size;
        let filter = Filter::try_from(options.algorithm)?;
        check_dimensions(width, height)?;
        ensure!(
            sw > 0
                && sh > 0
                && u64::from(sw) * u64::from(sh) <= 64_000_000
                && u64::from(width) * u64::from(sh) <= 64_000_000,
            "Input or intermediate image exceeds processing limits"
        );
        Ok(Self {
            source_size,
            target_size,
            horizontal: Weights::new(sw, width, filter),
            vertical: Weights::new(sh, height, filter),
            options,
        })
    }

    pub(super) fn resize(&self, source: SubImage<&RgbaImage>) -> RgbaImage {
        debug_assert_eq!(source.dimensions(), self.source_size);
        let (sw, sh) = self.source_size;
        let (width, height) = self.target_size;
        let options = self.options;
        let colors = match options.color_space {
            ColorSpace::Srgb => &SRGB,
            ColorSpace::Linear => &*LINEAR,
        };
        let (x, y) = source.offsets();
        let start = x as usize * 4;
        let end = start + sw as usize * 4;
        let mut input = Vec::with_capacity(sw as usize * sh as usize);
        let convert = |p: &[u8; 4]| {
            let alpha = SRGB[usize::from(p[3])];
            let mut value = [0.0; 4];
            for (out, byte) in value[..3].iter_mut().zip(&p[..3]) {
                let x = colors[usize::from(*byte)];
                *out = if options.premultiplied_alpha {
                    x * alpha
                } else {
                    x
                };
            }
            value[3] = alpha;
            value
        };
        // По строкам Vec получает точную длину расширения; flat_map здесь замедлял уменьшение изображений.
        for row in source
            .inner()
            .as_raw()
            .chunks_exact(source.inner().width() as usize * 4)
            .skip(y as usize)
            .take(sh as usize)
        {
            input.extend(row[start..end].as_chunks::<4>().0.iter().map(convert));
        }
        let mut intermediate = vec![[0.0_f32; 4]; width as usize * sh as usize];
        intermediate
            .par_chunks_mut(width as usize)
            .zip(input.par_chunks(sw as usize))
            // Мелкие кадры обрабатываются целиком: планирование строк дороже самой фильтрации.
            .with_min_len((16_384 / width as usize).max(1))
            .for_each(|(row, source)| {
                for (out, (start, weights)) in row.iter_mut().zip(self.horizontal.iter()) {
                    let source = &source[start..start + weights.len()];
                    // Каналы независимы; порядок умножений и сложений каждого канала сохраняется без FMA.
                    let mut value = f32x4::splat(0.0);
                    for (pixel, &weight) in source.iter().zip(weights) {
                        value += f32x4::from_array(*pixel) * f32x4::splat(weight);
                    }
                    *out = value.to_array();
                }
            });
        drop(input);
        let mut result = RgbaImage::new(width, height);
        result
            .as_mut()
            .par_chunks_mut(width as usize * 4)
            .zip(self.vertical.par_iter())
            .with_min_len((16_384 / width as usize).max(1))
            .for_each(|(row, (start, weights))| {
                const BLOCK: usize = 64;
                let stride = width as usize;
                let source = &intermediate[start * stride..(start + weights.len()) * stride];
                // Блок помещается на стеке; внутренний цикл читает каналы подряд и векторизуется без переассоциации сумм.
                for (block, pixels) in row.chunks_mut(BLOCK * 4).enumerate() {
                    let len = pixels.len() / 4;
                    let x = block * BLOCK;
                    let mut values = [[0.0_f32; 4]; BLOCK];
                    let values = &mut values[..len];
                    for (source_row, &weight) in source.chunks_exact(stride).zip(weights) {
                        let input = &source_row[x..x + len];
                        for (out, &value) in values
                            .as_flattened_mut()
                            .iter_mut()
                            .zip(input.as_flattened())
                        {
                            *out += value * weight;
                        }
                    }
                    match options.color_space {
                        ColorSpace::Srgb => {
                            encode_pixels::<false>(pixels, values, options.premultiplied_alpha);
                        }
                        ColorSpace::Linear => {
                            encode_pixels::<true>(pixels, values, options.premultiplied_alpha);
                        }
                    }
                }
            });
        if options.sharpening > 0.0 {
            let blurred = imageops::blur(&result, options.sharpening_radius);
            for (pixel, blur) in result.pixels_mut().zip(blurred.pixels()) {
                for channel in 0..3 {
                    let diff = f32::from(pixel[channel]) - f32::from(blur[channel]);
                    if diff.abs() > f32::from(options.sharpening_threshold) {
                        pixel[channel] = (f32::from(pixel[channel]) + diff * options.sharpening)
                            .round()
                            .clamp(0.0, 255.0) as u8;
                    }
                }
            }
        }
        result
    }
}

fn encode_pixels<const LINEAR: bool>(
    pixels: &mut [u8],
    values: &[[f32; 4]],
    premultiplied_alpha: bool,
) {
    // Константный параметр исключает powf из sRGB-пути даже при спекулятивной векторизации LLVM.
    for (pixel, value) in pixels.as_chunks_mut::<4>().0.iter_mut().zip(values) {
        let alpha = value[3].clamp(0.0, 1.0);
        for (out, mut v) in pixel[..3].iter_mut().zip(value[..3].iter().copied()) {
            if premultiplied_alpha {
                v = if alpha > 1e-6 { v / alpha } else { 0.0 };
            }
            if LINEAR {
                v = from_linear(v.clamp(0.0, 1.0));
            }
            *out = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        }
        pixel[3] = (alpha * 255.0).round() as u8;
    }
}

fn to_linear(v: f32) -> f32 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}
fn from_linear(v: f32) -> f32 {
    if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}
fn sinc(x: f32) -> f32 {
    if x.abs() < 1e-6 {
        1.0
    } else {
        (PI * x).sin() / (PI * x)
    }
}
fn kernel(x: f32, algorithm: Filter) -> f32 {
    let x = x.abs();
    match algorithm {
        Filter::Nearest => {
            if x <= 0.5 {
                1.0
            } else {
                0.0
            }
        }
        Filter::Bilinear => (1.0 - x).max(0.0),
        Filter::Lanczos2 | Filter::Lanczos3 => {
            let radius = if algorithm == Filter::Lanczos2 {
                2.0
            } else {
                3.0
            };
            if x < radius {
                sinc(x) * sinc(x / radius)
            } else {
                0.0
            }
        }
        Filter::CatmullRom | Filter::Mitchell => {
            let (b, c) = if algorithm == Filter::Mitchell {
                (1.0 / 3.0, 1.0 / 3.0)
            } else {
                (0.0, 0.5)
            };
            if x < 1.0 {
                ((12.0 - 9.0 * b - 6.0 * c) * x * x * x
                    + (-18.0 + 12.0 * b + 6.0 * c) * x * x
                    + 6.0
                    - 2.0 * b)
                    / 6.0
            } else if x < 2.0 {
                ((-b - 6.0 * c) * x * x * x
                    + (6.0 * b + 30.0 * c) * x * x
                    + (-12.0 * b - 48.0 * c) * x
                    + 8.0 * b
                    + 24.0 * c)
                    / 6.0
            } else {
                0.0
            }
        }
    }
}
#[cfg(test)]
mod tests;
