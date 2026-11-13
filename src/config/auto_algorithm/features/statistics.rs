use image::Rgba;
use serde::Serialize;

use super::Content;

#[derive(Debug, Clone, Copy, Serialize)]
pub(super) struct PixelStats {
    samples: u32,
    visible: u32,
    color_weight: u64,
    flat_weight: u64,
    varying_weight: u64,
    sharp_weight: u64,
    rough_weight: u64,
    curvature_weight: u64,
    near_energy: u64,
    far_energy: u64,
    alpha_pairs: u32,
    alpha_edges: u32,
    alpha_varying: u32,
    alpha_curves: u32,
    alpha_rough: u32,
    soft_alpha: u32,
    minimum: [u8; 3],
    maximum: [u8; 3],
    minimum_alpha: u8,
    maximum_alpha: u8,
}

impl PixelStats {
    pub(super) const fn empty() -> Self {
        Self {
            samples: 0,
            visible: 0,
            color_weight: 0,
            flat_weight: 0,
            varying_weight: 0,
            sharp_weight: 0,
            rough_weight: 0,
            curvature_weight: 0,
            near_energy: 0,
            far_energy: 0,
            alpha_pairs: 0,
            alpha_edges: 0,
            alpha_varying: 0,
            alpha_curves: 0,
            alpha_rough: 0,
            soft_alpha: 0,
            minimum: [255; 3],
            maximum: [0; 3],
            minimum_alpha: 255,
            maximum_alpha: 0,
        }
    }

    pub(super) fn sample(&mut self, pixel: Rgba<u8>) -> bool {
        self.samples += 1;
        self.minimum_alpha = self.minimum_alpha.min(pixel[3]);
        self.maximum_alpha = self.maximum_alpha.max(pixel[3]);
        if pixel[3] == 0 {
            return false;
        }
        self.visible += 1;
        self.soft_alpha += u32::from(pixel[3] < 255);
        for ((minimum, maximum), &channel) in self
            .minimum
            .iter_mut()
            .zip(&mut self.maximum)
            .zip(&pixel.0[..3])
        {
            *minimum = (*minimum).min(channel);
            *maximum = (*maximum).max(channel);
        }
        true
    }

    pub(super) fn merge(&mut self, other: &Self) {
        self.samples += other.samples;
        self.visible += other.visible;
        self.color_weight += other.color_weight;
        self.flat_weight += other.flat_weight;
        self.varying_weight += other.varying_weight;
        self.sharp_weight += other.sharp_weight;
        self.rough_weight += other.rough_weight;
        self.curvature_weight += other.curvature_weight;
        self.near_energy += other.near_energy;
        self.far_energy += other.far_energy;
        self.alpha_pairs += other.alpha_pairs;
        self.alpha_edges += other.alpha_edges;
        self.alpha_varying += other.alpha_varying;
        self.alpha_curves += other.alpha_curves;
        self.alpha_rough += other.alpha_rough;
        self.soft_alpha += other.soft_alpha;
        self.minimum_alpha = self.minimum_alpha.min(other.minimum_alpha);
        self.maximum_alpha = self.maximum_alpha.max(other.maximum_alpha);
        for (minimum, value) in self.minimum.iter_mut().zip(other.minimum) {
            *minimum = (*minimum).min(value);
        }
        for (maximum, value) in self.maximum.iter_mut().zip(other.maximum) {
            *maximum = (*maximum).max(value);
        }
    }

    pub(super) fn content(&self) -> Option<Content> {
        if self.visible == 0
            || (self.minimum == self.maximum && self.minimum_alpha == self.maximum_alpha)
        {
            return None;
        }
        Some(if self.has_flat_edges() {
            Content::Flat
        } else if self.is_smooth() {
            Content::Smooth
        } else if self.is_unstructured() {
            Content::Unstructured
        } else if self.is_detailed() {
            Content::Detailed
        } else {
            Content::Unknown
        })
    }

    pub(super) fn pair(&mut self, a: Rgba<u8>, b: Rgba<u8>) {
        if a[3].max(b[3]) > 0 {
            self.alpha_pairs += 1;
            self.alpha_edges += u32::from(a[3].abs_diff(b[3]) >= 64);
            self.alpha_varying += u32::from(a[3] != b[3]);
        }
        let weight = u64::from(a[3].min(b[3]));
        let difference = a.0[..3]
            .iter()
            .zip(&b.0[..3])
            .map(|(&a, &b)| a.abs_diff(b))
            .max()
            .unwrap_or(0);
        self.color_weight += weight;
        self.flat_weight += weight * u64::from(difference <= 4);
        self.varying_weight += weight * u64::from(difference > 0);
        self.sharp_weight += weight * u64::from(difference >= 64);
    }

    pub(super) fn curvature(&mut self, a: Rgba<u8>, b: Rgba<u8>, c: Rgba<u8>) {
        if a[3].max(b[3]).max(c[3]) > 0 {
            self.alpha_curves += 1;
            self.alpha_rough +=
                u32::from((i16::from(a[3]) - 2 * i16::from(b[3]) + i16::from(c[3])).abs() >= 20);
        }
        let weight = u64::from(a[3].min(b[3]).min(c[3]));
        let mut rough = false;
        let mut near = 0_u32;
        let mut far = 0_u32;
        for ((&a, &b), &c) in a.0[..3].iter().zip(&b.0[..3]).zip(&c.0[..3]) {
            rough |= (i16::from(a) - 2 * i16::from(b) + i16::from(c)).abs() >= 20;
            near += u32::from(a.abs_diff(b)).pow(2);
            far += u32::from(a.abs_diff(c)).pow(2);
        }
        self.curvature_weight += weight;
        self.rough_weight += weight * u64::from(rough);
        self.near_energy += weight * u64::from(near);
        self.far_energy += weight * u64::from(far);
    }

    // Пороги консервативные и требуют калибровки на игровых ассетах; один признак не определяет класс.
    pub(super) fn has_flat_edges(&self) -> bool {
        self.visible >= 8
            && self.color_weight > 0
            && self.flat_weight * 100 >= self.color_weight * 60
            && (self.sharp_weight * 100 >= self.color_weight
                || self.alpha_edges * 100 >= self.alpha_pairs.max(1))
    }

    /// У плоской графики большая часть кривизны объясняется резкими границами; мелкая фактура даёт дополнительную кривизну.
    pub(super) fn is_flat_graphic(&self) -> bool {
        self.has_flat_edges()
            && self.rough_weight * self.color_weight
                <= 4 * self.sharp_weight * self.curvature_weight
    }

    pub(super) const fn is_detailed(&self) -> bool {
        self.visible >= 32
            && self.curvature_weight > 0
            && self.rough_weight * 100 >= self.curvature_weight * 30
            && self.flat_weight * 100 < self.color_weight * 35
    }

    /// Слабоконтрастная вариация без выраженной связи на расстоянии одного и двух пикселей не доказывает наличие полезных деталей.
    pub(super) const fn is_unstructured(&self) -> bool {
        self.visible >= 128
            && self.is_detailed()
            && self.near_energy > 0
            && self.sharp_weight * 100 < self.color_weight
            && self.far_energy * 100 >= self.near_energy * 85
            && self.far_energy * 100 <= self.near_energy * 115
    }

    pub(super) fn is_smooth(&self) -> bool {
        self.visible >= 32
            && self.curvature_weight > 0
            && self.rough_weight * 100 <= self.curvature_weight
            && self.sharp_weight == 0
            && self.alpha_edges == 0
            && ((self.varying_weight * 10 >= self.color_weight
                && self
                    .minimum
                    .iter()
                    .zip(self.maximum)
                    .any(|(&low, high)| high.saturating_sub(low) >= 32))
                || (self.soft_alpha * 10 >= self.visible
                    && self.alpha_varying > 0
                    && self.alpha_rough * 100 <= self.alpha_curves
                    && self.flat_weight * 100 >= self.color_weight * 90))
    }
}
