mod features;

use super::{Algorithm, Category, Scale};
use features::{Consensus, Content};

pub use features::ImageFeatures;

/// Фиксированное решение не требует чтения пикселей; адаптивное уточняет исходную рекомендацию.
#[derive(Debug, Clone, Copy)]
pub(super) enum Selection {
    Fixed(Option<AlgorithmRecommendation>),
    Adaptive {
        fallback: Option<AlgorithmRecommendation>,
        small_frame: bool,
        purpose: Purpose,
    },
}

impl Selection {
    pub(super) fn new(
        path: &str,
        category: Category,
        frame_size: Option<(u32, u32)>,
        scale: Scale,
    ) -> Self {
        let purpose = Purpose::new(path, category);
        let fallback = purpose.recommend(frame_size);
        if matches!(purpose, Purpose::Mask) {
            return Self::Fixed(fallback);
        }
        let Some((width, height)) = frame_size else {
            return Self::Fixed(fallback);
        };
        let target = (scale.dimension(width), scale.dimension(height));
        if target == (width, height) {
            Self::Fixed(Some(AlgorithmRecommendation::new(
                Algorithm::Nearest,
                "unchanged pixel dimensions",
            )))
        } else if target.0 < width || target.1 < height {
            Self::Fixed(Some(AlgorithmRecommendation::new(
                Algorithm::Lanczos3,
                "downsampling with antialiasing",
            )))
        } else {
            Self::Adaptive {
                fallback,
                small_frame: width <= 32 && height <= 32,
                purpose,
            }
        }
    }

    pub(super) const fn needs_features(self) -> bool {
        matches!(self, Self::Adaptive { .. })
    }

    pub(super) fn resolve(
        self,
        features: Option<&ImageFeatures>,
    ) -> Option<AlgorithmRecommendation> {
        let (fallback, small_frame, purpose) = match self {
            Self::Fixed(choice) => return choice,
            Self::Adaptive {
                fallback,
                small_frame,
                purpose,
            } => (fallback, small_frame, purpose),
        };
        let Some(features) = features else {
            return fallback;
        };
        let content = match features.consensus() {
            Consensus::Mixed => {
                return fallback.map(|choice| {
                    AlgorithmRecommendation::new(
                        choice.algorithm,
                        "mixed sprite-frame structure; texture purpose",
                    )
                });
            }
            Consensus::Agreed(Content::Unstructured) => {
                return fallback.map(|choice| {
                    AlgorithmRecommendation::new(
                        choice.algorithm,
                        "unstructured low-contrast detail; texture purpose",
                    )
                });
            }
            Consensus::Agreed(content) => content,
            Consensus::Neutral => return fallback,
        };
        let (algorithm, reason) = if content == Content::Flat
            && (!matches!(purpose, Purpose::Artwork) || features.is_flat_graphic())
        {
            (
                Algorithm::Mitchell,
                "flat regions with strong color or alpha edges",
            )
        } else if content == Content::Smooth {
            (Algorithm::Bilinear, "smooth color or alpha gradient")
        } else if content == Content::Detailed {
            if small_frame {
                (Algorithm::CatmullRom, "detail in a small sprite frame")
            } else if features.has_small_content() {
                (
                    Algorithm::CatmullRom,
                    "small detailed content on a transparent canvas",
                )
            } else {
                (Algorithm::Lanczos3, "distributed fine detail")
            }
        } else {
            return fallback;
        };
        Some(AlgorithmRecommendation::new(algorithm, reason))
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) enum Purpose {
    Mask,
    Frame,
    Control,
    Artwork,
    Icon,
    Background,
    SpriteSheet,
}

impl Purpose {
    fn new(path: &str, category: Category) -> Self {
        let path = TexturePath::new(path);
        if category == Category::Mask
            || path.named(&["mask", "gradient"])
            || path.in_directory(&["masks", "gradients"])
        {
            Self::Mask
        } else if category == Category::Frame {
            Self::Frame
        } else if path.named(&[
            "button", "buttons", "frame", "border", "divider", "checkbox", "slider",
        ]) {
            Self::Control
        } else if path.in_directory(&[
            "portraits",
            "portrait",
            "event_pictures",
            "event_pictures_animated",
            "loadingscreens",
            "loading_screens",
            "illustrations",
            "ascension_perks",
            "relics",
            "deposits",
            "decisions",
            "planet_classes",
            "planet_types",
        ]) || path.named(&["portrait", "illustration"])
        {
            Self::Artwork
        } else if path.in_directory(&["buttons", "frames", "cursors", "dividers"])
            || path.named(&["icon", "icons"])
        {
            Self::Icon
        } else {
            match category {
                Category::Mask => Self::Mask,
                Category::Frame => Self::Frame,
                Category::Icon => Self::Icon,
                Category::Artwork | Category::Background => Self::Background,
                Category::SpriteSheet => Self::SpriteSheet,
            }
        }
    }

    fn recommend(self, frame_size: Option<(u32, u32)>) -> Option<AlgorithmRecommendation> {
        let (algorithm, reason) = match self {
            Self::Mask => (Algorithm::Bilinear, "mask or gradient"),
            Self::Frame => (Algorithm::CatmullRom, "tiled UI frame"),
            Self::Control => (Algorithm::CatmullRom, "UI control or frame"),
            Self::Artwork => (Algorithm::Lanczos3, "artwork or illustrated icon"),
            Self::Icon => (Algorithm::CatmullRom, "UI control or icon"),
            Self::Background => (Algorithm::Lanczos3, "artwork or background"),
            Self::SpriteSheet => {
                let (width, height) = frame_size?;
                if width <= 256 && height <= 256 {
                    (
                        Algorithm::CatmullRom,
                        "small sprite frame (up to 256 x 256)",
                    )
                } else {
                    (Algorithm::Lanczos3, "large sprite frame")
                }
            }
        };
        Some(AlgorithmRecommendation::new(algorithm, reason))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlgorithmRecommendation {
    pub algorithm: Algorithm,
    pub reason: &'static str,
}

impl AlgorithmRecommendation {
    const fn new(algorithm: Algorithm, reason: &'static str) -> Self {
        Self { algorithm, reason }
    }
}

struct TexturePath<'a> {
    parent: &'a str,
    stem: &'a str,
}

impl<'a> TexturePath<'a> {
    fn new(path: &'a str) -> Self {
        let (parent, file) = path.rsplit_once('/').unwrap_or(("", path));
        Self {
            parent,
            stem: file.rsplit_once('.').map_or(file, |(stem, _)| stem),
        }
    }

    fn named(&self, names: &[&str]) -> bool {
        self.stem
            .split(['_', '-', ' '])
            .any(|word| names.iter().any(|name| word.eq_ignore_ascii_case(name)))
    }

    fn in_directory(&self, directories: &[&str]) -> bool {
        self.parent.split('/').any(|part| {
            directories
                .iter()
                .any(|directory| part.eq_ignore_ascii_case(directory))
        })
    }
}

#[cfg(test)]
mod tests;
