use anyhow::Result;
use image::RgbaImage;
use serde::Serialize;

use super::{
    Algorithm, AlgorithmRecommendation, ImageFeatures, Scale, TextureOptions,
    auto_algorithm::Selection,
};

#[derive(Debug, Clone, Copy)]
pub struct TextureProcessing {
    pub options: TextureOptions,
    pub recommendation: Option<AlgorithmRecommendation>,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct AutoPolicy {
    pub(super) algorithm: bool,
    pub(super) sharpening: bool,
}

/// Разрешённые правила не зависят от пикселей и переиспользуются при анализе, предпросмотре и сборке.
#[derive(Debug, Clone, Copy)]
pub struct TexturePolicy {
    pub(crate) scale: Scale,
    pub(crate) automatic: AutoPolicy,
    pub(super) options: TextureOptions,
    pub(super) selection: Selection,
}

impl TexturePolicy {
    pub(crate) fn uses_neural(self) -> bool {
        self.scale.get() > 1.0 && self.processing(None).options.algorithm == Algorithm::RealPlksr2x
    }

    pub(crate) fn processing(self, features: Option<&ImageFeatures>) -> TextureProcessing {
        let mut options = self.options;
        let recommendation = self.selection.resolve(features);
        if let Some(choice) = recommendation {
            options.algorithm = choice.algorithm;
        }
        if options.algorithm == Algorithm::RealPlksr2x {
            options.independent_frames = true;
        }
        if self.automatic.sharpening {
            let limit = match options.algorithm {
                Algorithm::Bilinear | Algorithm::Nearest | Algorithm::RealPlksr2x => 0.0,
                Algorithm::Mitchell => 0.15,
                _ if features.is_some_and(ImageFeatures::limits_sharpening) => 0.15,
                _ => options.sharpening,
            };
            options.sharpening = options.sharpening.min(limit);
        }
        TextureProcessing {
            options,
            recommendation,
        }
    }

    pub(crate) fn needs_pixel_analysis(self) -> bool {
        self.selection.needs_features()
            || (self.automatic.sharpening
                && self.options.sharpening > 0.15
                && !matches!(
                    self.processing(None).options.algorithm,
                    Algorithm::Bilinear
                        | Algorithm::Nearest
                        | Algorithm::Mitchell
                        | Algorithm::RealPlksr2x
                ))
    }

    /// # Errors
    /// Возвращает ошибку раскладки кадров, если для разрешения правил нужны пиксели.
    pub(crate) fn analyze(self, source: &RgbaImage, frames: u32) -> Result<Option<ImageFeatures>> {
        self.needs_pixel_analysis()
            .then(|| ImageFeatures::analyze(source, frames))
            .transpose()
    }

    pub(crate) const fn with_algorithm(mut self, algorithm: Algorithm) -> Self {
        self.options.algorithm = algorithm;
        self.automatic.algorithm = false;
        self.selection = Selection::Fixed(None);
        self
    }
}
