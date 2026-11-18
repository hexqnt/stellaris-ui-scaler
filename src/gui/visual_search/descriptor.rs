use anyhow::{Result, ensure};
use image::{GenericImageView, RgbaImage, imageops};

use crate::config::Scale;

const SIDE: u32 = 12;
const CELLS: usize = (SIDE * SIDE) as usize;
const MIN_DEVIATION: f32 = 0.05;

type Colors = [[f32; 3]; CELLS];
type Luminance = [f32; CELLS];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Region {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Region {
    // На скриншоте вырезка обычно плотнее границ исходной текстуры.
    pub(super) fn opaque_bounds(self, image: &RgbaImage) -> Option<Self> {
        let (mut left, mut top) = (self.x + self.width, self.y + self.height);
        let (mut right, mut bottom) = (self.x, self.y);
        for (x, y, pixel) in image.view(self.x, self.y, self.width, self.height).pixels() {
            if pixel[3] >= 16 {
                left = left.min(self.x + x);
                top = top.min(self.y + y);
                right = right.max(self.x + x + 1);
                bottom = bottom.max(self.y + y + 1);
            }
        }
        (left < right && top < bottom).then(|| Self {
            x: left,
            y: top,
            width: right - left,
            height: bottom - top,
        })
    }

    pub(super) fn whole(image: &RgbaImage) -> Self {
        Self {
            x: 0,
            y: 0,
            width: image.width(),
            height: image.height(),
        }
    }
}

// Альфа сохраняется, чтобы подставлять фон запроса без повторного декодирования исходника.
pub(super) struct Descriptor {
    pixels: [[f32; 4]; CELLS],
    dimensions: [u32; 2],
}

/// Проверенный запрос со статистикой, общей для сравнения со всеми кандидатами.
pub(super) struct Query {
    colors: Colors,
    luminance: Luminance,
    background: [f32; 3],
    histogram: Histogram,
    dimensions: [u32; 2],
}

fn luma([r, g, b]: [f32; 3]) -> f32 {
    0.2126f32.mul_add(r, 0.7152f32.mul_add(g, 0.0722 * b))
}

fn normalized(colors: &Colors) -> Luminance {
    let mut values = colors.map(luma);
    let mean = values.iter().sum::<f32>() / CELLS as f32;
    let variance = values
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f32>()
        / CELLS as f32;
    let deviation = variance.sqrt().max(MIN_DEVIATION);
    for value in &mut values {
        *value = (*value - mean) / deviation;
    }
    values
}

impl Query {
    pub fn new(image: &RgbaImage) -> Result<Self> {
        ensure!(image.width() > 0 && image.height() > 0, "Empty query image");
        ensure!(
            image.pixels().any(|pixel| pixel[3] >= 16),
            "Query image is fully transparent"
        );
        let descriptor = Descriptor::new(image, Region::whole(image));
        let mut background = [0.0; 3];
        let mut weight = 0.0f32;
        for (i, pixel) in descriptor.pixels.iter().enumerate() {
            let x = i % SIDE as usize;
            let y = i / SIDE as usize;
            if x == 0 || y == 0 || x == SIDE as usize - 1 || y == SIDE as usize - 1 {
                for (sum, channel) in background.iter_mut().zip(&pixel[..3]) {
                    *sum = channel.mul_add(pixel[3], *sum);
                }
                weight += pixel[3];
            }
        }
        for channel in &mut background {
            *channel /= weight.max(1.0);
        }
        let colors = descriptor.composite(background);
        Ok(Self {
            colors,
            luminance: normalized(&colors),
            background,
            histogram: Histogram::new(&colors),
            dimensions: descriptor.dimensions,
        })
    }
}

impl Descriptor {
    pub fn new(image: &RgbaImage, region: Region) -> Self {
        let view = image.view(region.x, region.y, region.width, region.height);
        let small = imageops::resize(&*view, SIDE, SIDE, imageops::FilterType::Triangle);
        let mut pixels = [[0.0; 4]; CELLS];
        for (pixel, rgba) in pixels.iter_mut().zip(small.pixels()) {
            *pixel = rgba.0.map(|v| f32::from(v) / 255.0);
        }
        Self {
            pixels,
            dimensions: [region.width, region.height],
        }
    }

    fn composite(&self, background: [f32; 3]) -> Colors {
        self.pixels.map(|pixel| {
            std::array::from_fn(|i| pixel[i].mul_add(pixel[3], background[i] * (1.0 - pixel[3])))
        })
    }

    pub fn distance(&self, query: &Query, scale: Scale) -> f32 {
        // Участие фона сохраняет силуэт прозрачных иконок при сравнении со скриншотом.
        let colors = self.composite(query.background);
        let luminance = normalized(&colors);
        let shape = luminance
            .iter()
            .zip(query.luminance)
            .map(|(a, b)| (a - b).powi(2))
            .sum::<f32>()
            / (4 * CELLS) as f32;
        let spatial_color = colors
            .iter()
            .zip(&query.colors)
            .map(|(a, b)| a.iter().zip(b).map(|(a, b)| (a - b).powi(2)).sum::<f32>())
            .sum::<f32>()
            / (3 * CELLS) as f32;
        let palette = Histogram::new(&colors).distance(&query.histogram);
        let color = 0.7f32.mul_add(palette, 0.3 * spatial_color);
        let size = self.size_distance(query, scale);
        // Размер и палитра важнее нормализованной формы; все три оценки ограничены диапазоном 0..=1.
        0.5f32.mul_add(size, 0.35f32.mul_add(color, 0.15 * shape.min(1.0)))
    }

    fn size_distance(&self, query: &Query, scale: Scale) -> f32 {
        let error = |dimensions: [u32; 2]| {
            dimensions
                .iter()
                .zip(query.dimensions)
                .map(|(&source, target)| (source as f32 / target as f32).log2().abs())
                .sum::<f32>()
                * 0.5
        };
        // Вырезка может быть снята как с исходного интерфейса, так и с обработанного текущим профилем.
        let difference =
            error(self.dimensions).min(error(self.dimensions.map(|value| scale.dimension(value))));
        difference / (1.0 + difference)
    }
}

const COLOR_BINS: usize = 4;
const HISTOGRAM_SIZE: usize = COLOR_BINS * COLOR_BINS * COLOR_BINS;

struct Histogram([f32; HISTOGRAM_SIZE]);

impl Histogram {
    fn new(colors: &Colors) -> Self {
        let mut bins = [0.0; HISTOGRAM_SIZE];
        for color in colors {
            let positions = color.map(|channel| channel * (COLOR_BINS - 1) as f32);
            let lower = positions.map(|position| position as usize);
            let fractions = std::array::from_fn::<_, 3, _>(|i| positions[i] - lower[i] as f32);
            // Трилинейное распределение сглаживает границы цветовых корзин.
            for red in 0..=1 {
                for green in 0..=1 {
                    for blue in 0..=1 {
                        let offsets = [red, green, blue];
                        let indices = std::array::from_fn::<_, 3, _>(|i| {
                            (lower[i] + offsets[i]).min(COLOR_BINS - 1)
                        });
                        let weight = offsets
                            .iter()
                            .zip(fractions)
                            .map(|(&offset, fraction)| {
                                if offset == 0 {
                                    1.0 - fraction
                                } else {
                                    fraction
                                }
                            })
                            .product::<f32>();
                        let [r, g, b] = indices;
                        bins[(r * COLOR_BINS + g) * COLOR_BINS + b] += weight / CELLS as f32;
                    }
                }
            }
        }
        // Корни сохраняются один раз для запроса и переиспользуются при вычислении расстояния Хеллингера.
        Self(bins.map(f32::sqrt))
    }

    fn distance(&self, other: &Self) -> f32 {
        (0.5 * self
            .0
            .iter()
            .zip(other.0)
            .map(|(a, b)| (a - b).powi(2))
            .sum::<f32>())
        .sqrt()
        .min(1.0)
    }
}
