mod statistics;

use anyhow::{Result, ensure};
use image::RgbaImage;
use serde::Serialize;

use statistics::PixelStats;

const SAMPLE_BUDGET: u32 = 65_536;
const FRAME_BUDGET: u32 = 16;

/// Тип содержимого описывает наблюдаемые признаки, а не семантический класс изображения.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Content {
    Flat,
    Smooth,
    Detailed,
    Unstructured,
    Unknown,
}

#[derive(Debug, Default, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Consensus {
    #[default]
    Neutral,
    Agreed(Content),
    Mixed,
}

#[derive(Debug, Default, Clone, Copy, Serialize)]
struct FrameSummary {
    sampled: u32,
    contributing: u32,
    flat_edges: u32,
    flat_graphics: u32,
    smooth: u32,
    detailed: u32,
    unstructured: u32,
    unknown: u32,
    consensus: Consensus,
}

impl FrameSummary {
    fn record(&mut self, stats: &PixelStats) {
        self.sampled += 1;
        self.flat_edges += u32::from(stats.has_flat_edges());
        self.flat_graphics += u32::from(stats.is_flat_graphic());
        if let Some(content) = stats.content() {
            self.contributing += 1;
            match content {
                Content::Flat => {}
                Content::Smooth => self.smooth += 1,
                Content::Detailed => self.detailed += 1,
                Content::Unstructured => self.unstructured += 1,
                Content::Unknown => self.unknown += 1,
            }
            self.consensus = match self.consensus {
                Consensus::Neutral => Consensus::Agreed(content),
                Consensus::Agreed(previous) if previous == content => self.consensus,
                Consensus::Agreed(_) | Consensus::Mixed => Consensus::Mixed,
            };
        }
    }
}

/// Ограниченная выборка соседних пикселей; решения согласуются между видимыми кадрами.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct ImageFeatures {
    #[serde(flatten)]
    stats: PixelStats,
    frame_summary: FrameSummary,
    content_size: Option<(u32, u32)>,
}

impl ImageFeatures {
    /// # Errors
    /// Возвращает ошибку пустого изображения или недопустимого числа кадров.
    pub fn analyze(image: &RgbaImage, frames: u32) -> Result<Self> {
        ensure!(
            frames > 0 && image.width() >= frames && image.height() > 0,
            "Invalid frame layout for image analysis"
        );
        let width = image.width() / frames;
        let sampled_frames = frames.min(FRAME_BUDGET);
        let budget = SAMPLE_BUDGET / sampled_frames;
        let columns = width.min(256).min(budget);
        let rows = image.height().min(budget / columns);
        let mut features = Self {
            stats: PixelStats::empty(),
            frame_summary: FrameSummary::default(),
            content_size: None,
        };
        for frame in 0..sampled_frames {
            let offset = position(frame, sampled_frames, frames) * width;
            let mut stats = PixelStats::empty();
            let mut bounds: Option<ContentBounds> = None;
            for row in 0..rows {
                let y = position(row, rows, image.height());
                for column in 0..columns {
                    let x = position(column, columns, width);
                    let pixel = *image.get_pixel(offset + x, y);
                    if stats.sample(pixel) {
                        bounds
                            .get_or_insert_with(|| ContentBounds::at(x, y))
                            .include(x, y);
                    }
                    // Соседи принадлежат тому же кадру; остаток справа от атласа не анализируется.
                    for (dx, dy, remaining) in [(1, 0, width - x), (0, 1, image.height() - y)] {
                        if remaining > 1 {
                            let neighbor = *image.get_pixel(offset + x + dx, y + dy);
                            stats.pair(pixel, neighbor);
                            if remaining > 2 {
                                stats.curvature(
                                    pixel,
                                    neighbor,
                                    *image.get_pixel(offset + x + 2 * dx, y + 2 * dy),
                                );
                            }
                        }
                    }
                }
            }
            if let Some(bounds) = bounds {
                let size = (
                    sampled_span(bounds.left, bounds.right, width, columns),
                    sampled_span(bounds.top, bounds.bottom, image.height(), rows),
                );
                features.content_size =
                    Some(features.content_size.map_or(size, |(width, height)| {
                        (width.max(size.0), height.max(size.1))
                    }));
            }
            features.frame_summary.record(&stats);
            features.stats.merge(&stats);
        }
        Ok(features)
    }

    pub(super) const fn consensus(&self) -> Consensus {
        self.frame_summary.consensus
    }

    pub(in crate::config) const fn has_flat_edges(&self) -> bool {
        self.frame_summary.flat_edges > 0
    }

    pub(in crate::config) const fn limits_sharpening(&self) -> bool {
        self.has_flat_edges() || self.frame_summary.unstructured > 0
    }

    pub(super) const fn is_flat_graphic(&self) -> bool {
        self.frame_summary.contributing > 0
            && self.frame_summary.flat_graphics == self.frame_summary.contributing
    }

    pub(super) const fn has_small_content(&self) -> bool {
        matches!(self.content_size, Some((width, height)) if width <= 32 && height <= 32)
    }
}

#[derive(Clone, Copy)]
struct ContentBounds {
    left: u32,
    right: u32,
    top: u32,
    bottom: u32,
}

impl ContentBounds {
    const fn at(x: u32, y: u32) -> Self {
        Self {
            left: x,
            right: x,
            top: y,
            bottom: y,
        }
    }

    fn include(&mut self, x: u32, y: u32) {
        self.left = self.left.min(x);
        self.right = self.right.max(x);
        self.top = self.top.min(y);
        self.bottom = self.bottom.max(y);
    }
}

fn sampled_span(low: u32, high: u32, extent: u32, samples: u32) -> u32 {
    if samples == extent {
        high - low + 1
    } else if samples > 1 {
        // При неполной выборке расширяем оценку на шаг сетки с каждой стороны; это не точная граница альфа-канала.
        let margin = (extent - 1).div_ceil(samples - 1);
        (high - low + 1)
            .saturating_add(margin.saturating_mul(2))
            .min(extent)
    } else {
        extent
    }
}

fn position(index: u32, count: u32, extent: u32) -> u32 {
    if count == 1 {
        extent / 2
    } else {
        // Деление ограничивает результат исходной размерностью u32.
        u32::try_from(u64::from(index) * u64::from(extent - 1) / u64::from(count - 1)).unwrap()
    }
}

#[cfg(test)]
mod tests;
