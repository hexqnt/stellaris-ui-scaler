use std::{num::NonZeroUsize, sync::Mutex};

use anyhow::{Context, Result};
use image::{GenericImage, GenericImageView, RgbaImage, SubImage};

use crate::config::{Algorithm, Scale, TextureOptions};

use super::{
    check_dimensions,
    realplksr::Model,
    resample::{self, Resampler},
    target_dimensions,
};

pub const PERIODIC_PADDING: u32 = 8;

#[derive(Clone, Copy)]
pub enum Boundary {
    Image,
    Periodic,
}

pub struct Processor {
    neural: Mutex<Option<Model>>,
    neural_threads: NonZeroUsize,
}

impl Processor {
    pub(crate) const fn with_neural_threads(neural_threads: NonZeroUsize) -> Self {
        Self {
            neural: Mutex::new(None),
            neural_threads,
        }
    }

    pub(crate) fn set_neural_threads(&mut self, threads: NonZeroUsize) {
        if self.neural_threads != threads {
            *self = Self::with_neural_threads(threads);
        }
    }

    pub(super) fn resize(
        &self,
        source: SubImage<&RgbaImage>,
        target: (u32, u32),
        options: &TextureOptions,
        boundary: Boundary,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<RgbaImage> {
        check()?;
        check_dimensions(source.width(), source.height())?;
        check_dimensions(target.0, target.1)?;
        if options.algorithm != Algorithm::RealPlksr2x {
            return Ok(Resampler::new(source.dimensions(), target, options)?.resize(source));
        }
        let finish = TextureOptions {
            algorithm: Algorithm::Lanczos3,
            ..*options
        };
        if target.0 <= source.width() && target.1 <= source.height() {
            return Ok(Resampler::new(source.dimensions(), target, &finish)?.resize(source));
        }
        check_dimensions(source.width() * 2, source.height() * 2)?;
        if source.pixels().all(|(_, _, pixel)| pixel[3] == 0) {
            return Ok(RgbaImage::new(target.0, target.1));
        }
        let native = {
            // Одна сессия и её входной буфер переиспользуются; сборка выделяет отдельный Processor каждой параллельной задаче.
            let mut model = self
                .neural
                .lock()
                .map_err(|_| anyhow::anyhow!("RealPLKSR session lock poisoned"))?;
            check()?;
            if model.is_none() {
                *model = Some(Model::load(self.neural_threads)?);
            }
            model
                .as_mut()
                .context("Missing RealPLKSR session")?
                .upscale(source, boundary, check)?
        };
        check()?;
        resample::resize(&native, target.0, target.1, &finish)
    }

    pub(crate) fn process(
        &self,
        source: &RgbaImage,
        scale: Scale,
        frames: u32,
        options: &TextureOptions,
        boundary: Boundary,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<RgbaImage> {
        let target = target_dimensions(source.width(), source.height(), frames, scale)?;
        // Кадры имеют целую ширину; остаток справа не входит в используемую область атласа.
        let source = source.view(0, 0, source.width() / frames * frames, source.height());
        let mut options = *options;
        if matches!(boundary, Boundary::Periodic) {
            options.sharpening = 0.0;
            if options.algorithm == Algorithm::RealPlksr2x && scale.get() <= 1.0 {
                options.algorithm = Algorithm::Lanczos3;
            }
        }
        let parts = if options.independent_frames || options.algorithm == Algorithm::RealPlksr2x {
            frames
        } else {
            1
        };
        if matches!(boundary, Boundary::Periodic) && options.algorithm != Algorithm::RealPlksr2x {
            Self::resize_periodic(
                source,
                (target.0 / parts, target.1),
                parts,
                scale,
                &options,
                check,
            )
        } else {
            self.resize_frames(source, target, parts, &options, boundary, check)
        }
    }

    pub(super) fn resize_frames(
        &self,
        source: SubImage<&RgbaImage>,
        target: (u32, u32),
        frames: u32,
        options: &TextureOptions,
        boundary: Boundary,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<RgbaImage> {
        let frame_width = source.width() / frames;
        let target_frame = (target.0 / frames, target.1);
        let resampler = (options.algorithm != Algorithm::RealPlksr2x)
            .then(|| Resampler::new((frame_width, source.height()), target_frame, options))
            .transpose()?;
        let resize = |input| {
            check()?;
            resampler.as_ref().map_or_else(
                || self.resize(input, target_frame, options, boundary, check),
                |filter| Ok(filter.resize(input)),
            )
        };
        if frames == 1 {
            return resize(source);
        }
        let mut result = RgbaImage::new(target.0, target.1);
        for frame in 0..frames {
            let input = source.view(frame * frame_width, 0, frame_width, source.height());
            result.copy_from(&resize(input)?, frame * target_frame.0, 0)?;
        }
        Ok(result)
    }

    fn resize_periodic(
        source: SubImage<&RgbaImage>,
        target: (u32, u32),
        parts: u32,
        scale: Scale,
        options: &TextureOptions,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<RgbaImage> {
        let (target_width, target_height) = target;
        let source_width = source.width() / parts;
        let padding = PERIODIC_PADDING;
        let scaled_padding = scale.dimension(padding);
        check_dimensions(source_width + padding * 2, source.height() + padding * 2)?;
        check_dimensions(
            target_width + scaled_padding * 2,
            target_height + scaled_padding * 2,
        )?;
        let mut output = RgbaImage::new(target_width * parts, target_height);
        let resampler = Resampler::new(
            (source_width + 2 * padding, source.height() + 2 * padding),
            (
                target_width + 2 * scaled_padding,
                target_height + 2 * scaled_padding,
            ),
            options,
        )?;
        for frame in 0..parts {
            check()?;
            let padded = RgbaImage::from_fn(
                source_width + padding * 2,
                source.height() + padding * 2,
                |x, y| {
                    let x = (x + source_width * padding - padding) % source_width;
                    let y = (y + source.height() * padding - padding) % source.height();
                    source.get_pixel(frame * source_width + x, y)
                },
            );
            let result = resampler.resize(padded.view(0, 0, padded.width(), padded.height()));
            let result = result.view(scaled_padding, scaled_padding, target_width, target_height);
            output.copy_from(&*result, frame * target_width, 0)?;
        }
        Ok(output)
    }
}

impl Default for Processor {
    fn default() -> Self {
        Self::with_neural_threads(
            crate::parallelism::CpuBudget::current().per_task(NonZeroUsize::MIN),
        )
    }
}
