// Размеры ограничены профилем и лимитом изображения; преобразования каналов насыщаются явно.
#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use std::{path::PathBuf, str::FromStr};

use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum Algorithm {
    Nearest,
    Bilinear,
    CatmullRom,
    Mitchell,
    Lanczos2,
    Lanczos3,
    #[serde(rename = "realplksr_2x")]
    #[value(name = "realplksr-2x")]
    RealPlksr2x,
}
impl Algorithm {
    pub const RESAMPLING: [Self; 6] = *Self::ALL.first_chunk::<6>().unwrap();

    pub const ALL: [Self; 7] = [
        Self::Nearest,
        Self::Bilinear,
        Self::CatmullRom,
        Self::Mitchell,
        Self::Lanczos2,
        Self::Lanczos3,
        Self::RealPlksr2x,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Nearest => "Nearest",
            Self::Bilinear => "Bilinear",
            Self::CatmullRom => "CatmullRom",
            Self::Mitchell => "Mitchell",
            Self::Lanczos2 => "Lanczos2",
            Self::Lanczos3 => "Lanczos3",
            Self::RealPlksr2x => "RealPLKSR 2x",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorSpace {
    Srgb,
    Linear,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Icon,
    SpriteSheet,
    Background,
    Mask,
    Artwork,
    Frame,
}
impl Category {
    pub const ALL: [Self; 6] = [
        Self::Icon,
        Self::SpriteSheet,
        Self::Background,
        Self::Mask,
        Self::Artwork,
        Self::Frame,
    ];
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Icon => "icon",
            Self::SpriteSheet => "sprite_sheet",
            Self::Background => "background",
            Self::Mask => "mask",
            Self::Artwork => "artwork",
            Self::Frame => "frame",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "f64", into = "f64")]
pub struct Scale(f64);
impl Scale {
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
    #[must_use]
    pub fn pixels(self, value: f64) -> f64 {
        (value * self.0).round_ties_even()
    }
    #[must_use]
    pub fn dimension(self, value: u32) -> u32 {
        self.pixels(f64::from(value)).max(1.0) as u32
    }
}

impl TryFrom<f64> for Scale {
    type Error = anyhow::Error;
    fn try_from(value: f64) -> Result<Self> {
        ensure!(
            (0.25..=8.0).contains(&value),
            "Scale must be finite and between 0.25 and 8"
        );
        Ok(Self(value))
    }
}
impl FromStr for Scale {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        Self::try_from(value.parse::<f64>()?)
    }
}
impl Default for Scale {
    fn default() -> Self {
        Self(1.86921)
    }
}
impl From<Scale> for f64 {
    fn from(value: Scale) -> Self {
        value.0
    }
}
// Один список полей связывает полные параметры, частичные правила и их применение.
macro_rules! texture_options {
    ($($field:ident: $kind:ty = $default:expr),+ $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
        #[serde(default, deny_unknown_fields)]
        pub struct TextureOptions {
            $(pub $field: $kind,)+
        }
        impl Default for TextureOptions {
            fn default() -> Self {
                Self { $($field: $default,)+ }
            }
        }

        #[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
        #[serde(default, deny_unknown_fields)]
        pub struct TextureRule {
            pub scale: Option<Scale>,
            $(pub $field: Option<$kind>,)+
        }
        impl TextureRule {
            pub(super) const fn apply(&self, options: &mut TextureOptions) {
                $(if let Some(value) = self.$field { options.$field = value; })+
            }
        }
    };
}

texture_options! {
    algorithm: Algorithm = Algorithm::Lanczos3,
    color_space: ColorSpace = ColorSpace::Srgb,
    premultiplied_alpha: bool = true,
    independent_frames: bool = true,
    sharpening: f32 = 0.45,
    sharpening_radius: f32 = 1.0,
    sharpening_threshold: u8 = 2,
}

/// Общая политика растровых вариантов; назначенные игрой или модом TTF сохраняются.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FontMode {
    #[default]
    Automatic,
    #[serde(rename = "truetype")]
    TrueType,
    Original,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct OutlinerOptions {
    pub compact: bool,
    pub width: u32,
    pub row_height: u32,
    pub title_height: u32,
}
impl Default for OutlinerOptions {
    fn default() -> Self {
        Self {
            compact: false,
            width: 600,
            row_height: 30,
            title_height: 40,
        }
    }
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FontRule {
    pub replace_bitmap: Option<bool>,
    pub family: Option<String>,
    pub scale: Option<Scale>,
    pub base_size: Option<FontSize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "f64", into = "f64")]
pub struct FontSize(f64);

impl TryFrom<f64> for FontSize {
    type Error = anyhow::Error;
    fn try_from(value: f64) -> Result<Self> {
        ensure!(
            (1.0..=512.0).contains(&value),
            "Font size must be finite and between 1 and 512"
        );
        Ok(Self(value))
    }
}
/// Настройки обнаружения и сохранённый порядок модов для проверки перед сканированием.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AutoSourceMods {
    pub user_data_dir: Option<PathBuf>,
    pub source_mods: Vec<PathBuf>,
}

pub const SHARPENING: std::ops::RangeInclusive<f32> = 0.0..=2.0;
pub const SHARPENING_RADIUS: std::ops::RangeInclusive<f32> = 0.1..=5.0;

impl TextureRule {
    pub(super) fn check(&self, key: &str) -> Result<()> {
        ensure!(
            self.sharpening
                .is_none_or(|value| SHARPENING.contains(&value)),
            "Invalid sharpening for {key}"
        );
        ensure!(
            self.sharpening_radius
                .is_none_or(|value| SHARPENING_RADIUS.contains(&value)),
            "Invalid sharpening radius for {key}"
        );
        Ok(())
    }
}
impl From<FontSize> for f64 {
    fn from(value: FontSize) -> Self {
        value.0
    }
}
