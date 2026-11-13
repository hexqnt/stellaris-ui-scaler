mod auto_algorithm;
mod texture_policy;
mod types;

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use texture_policy::AutoPolicy;

pub use auto_algorithm::{AlgorithmRecommendation, ImageFeatures};
pub(crate) use texture_policy::TexturePolicy;
pub use texture_policy::TextureProcessing;
pub use types::{
    Algorithm, AutoSourceMods, Category, ColorSpace, FontMode, FontRule, FontSize, OutlinerOptions,
    Scale, TextureOptions, TextureRule,
};

pub(crate) const OUTLINER_REFERENCE_SCALE: f64 = 1.3846;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Profile {
    pub name: String,
    pub mod_version: String,
    pub supported_version: Option<String>,
    pub picture: Option<PathBuf>,
    pub source_mods: Vec<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_source_mods: Option<AutoSourceMods>,
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub disabled_source_mods: BTreeSet<PathBuf>,
    pub resolution: [u32; 2],
    pub scale: Scale,
    pub font_scale: Option<Scale>,
    pub font_mode: FontMode,
    pub fonts: BTreeMap<String, FontRule>,
    pub reproduction: bool,
    pub auto_algorithm: bool,
    pub auto_sharpening: bool,
    pub textures: TextureOptions,
    pub outliner: OutlinerOptions,
    pub categories: BTreeMap<String, TextureRule>,
    pub directories: BTreeMap<String, TextureRule>,
    pub assets: BTreeMap<String, TextureRule>,
}
impl Profile {
    /// Применяет общий стиль ко всем группам, сохраняя индивидуальные размеры.
    pub fn apply_font_mode(&mut self, mode: FontMode) {
        self.font_mode = mode;
        self.fonts.retain(|_, rule| {
            rule.replace_bitmap = None;
            rule.family = None;
            *rule != FontRule::default()
        });
    }

    /// # Errors
    /// Возвращает ошибку чтения, TOML или параметров профиля.
    pub fn load(path: &Path) -> Result<Self> {
        let source = fs::read_to_string(path)
            .with_context(|| format!("Cannot read profile: {}", path.display()))?;
        let profile: Self = toml::from_str(&source)
            .with_context(|| format!("Cannot parse profile: {}", path.display()))?;
        profile
            .check()
            .with_context(|| format!("Invalid profile: {}", path.display()))?;
        Ok(profile)
    }
    /// # Errors
    /// Возвращает ошибку параметров, сериализации или записи.
    pub fn save(&self, path: &Path) -> Result<()> {
        self.check()?;
        let text = toml::to_string_pretty(self)?;
        crate::atomic_file::AtomicFile::prepare(path, |file| {
            file.write_all(text.as_bytes())?;
            file.sync_all()
        })
        .and_then(crate::atomic_file::AtomicFile::publish)
        .with_context(|| format!("Cannot save profile: {}", path.display()))?;
        Ok(())
    }
    /// # Errors
    /// Возвращает ошибку недопустимого параметра профиля.
    pub fn check(&self) -> Result<()> {
        ensure!(
            self.source_mods
                .iter()
                .chain(&self.disabled_source_mods)
                .chain(
                    self.auto_source_mods
                        .iter()
                        .flat_map(|settings| &settings.source_mods)
                )
                .all(|path| !path.as_os_str().is_empty()),
            "Source mod path must not be empty"
        );
        ensure!(
            self.auto_source_mods
                .as_ref()
                .and_then(|settings| settings.user_data_dir.as_ref())
                .is_none_or(|path| !path.as_os_str().is_empty()),
            "User data directory must not be empty"
        );
        ensure!(
            self.resolution.iter().all(|n| (640..=32768).contains(n)),
            "Resolution must be between 640 and 32768 pixels"
        );
        ensure!(
            !self.name.trim().is_empty() && crate::paths::descriptor_value(&self.name),
            "Invalid mod name"
        );
        for (label, version) in std::iter::once(("mod", &self.mod_version)).chain(
            self.supported_version
                .as_ref()
                .map(|version| ("supported game", version)),
        ) {
            ensure!(
                !version.trim().is_empty() && crate::paths::descriptor_value(version),
                "Invalid {label} version: use a non-empty value without quotes, backslashes or control characters"
            );
        }
        ensure!(
            self.textures.sharpening.is_finite() && (0.0..=2.0).contains(&self.textures.sharpening),
            "Sharpening must be between 0 and 2"
        );
        ensure!(
            self.textures.sharpening_radius.is_finite()
                && (0.1..=5.0).contains(&self.textures.sharpening_radius),
            "Sharpening radius must be between 0.1 and 5"
        );
        ensure!(
            (100..=2000).contains(&self.outliner.width)
                && (16..=100).contains(&self.outliner.row_height)
                && (20..=120).contains(&self.outliner.title_height),
            "Invalid reference outliner dimensions"
        );
        for (key, rule) in &self.categories {
            ensure!(
                Category::ALL.iter().any(|category| category.name() == key),
                "Unknown category: {key}"
            );
            rule.check(key)?;
        }
        for (key, rule) in self.directories.iter().chain(&self.assets) {
            crate::assets::safe_relative(key)?;
            rule.check(key)?;
        }
        for (name, rule) in &self.fonts {
            for value in std::iter::once(name.as_str()).chain(rule.family.as_deref()) {
                ensure!(
                    !value.is_empty()
                        && value
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || b"_-.".contains(&byte)),
                    "Invalid font identifier: {value}"
                );
            }
        }
        Ok(())
    }
    /// Порядок наследования: категория, папки от корня к листу, ассет.
    fn rules<'a>(
        &'a self,
        path: &'a str,
        category: Category,
    ) -> impl Iterator<Item = &'a TextureRule> {
        self.categories
            .get(category.name())
            .into_iter()
            .chain(
                path.match_indices('/')
                    .filter_map(|(end, _)| self.directories.get(&path[..end])),
            )
            .chain(self.assets.get(path))
    }

    #[must_use]
    pub fn texture_scale(&self, path: &str, category: Category) -> Scale {
        self.rules(path, category)
            .filter_map(|rule| rule.scale)
            .last()
            .unwrap_or(self.scale)
    }

    #[must_use]
    pub fn options(&self, path: &str, category: Category) -> TextureOptions {
        self.options_with_frame_size(path, category, None)
    }

    /// Выбирает параметры с учётом размеров одного кадра атласа.
    #[must_use]
    pub fn options_with_frame_size(
        &self,
        path: &str,
        category: Category,
        frame_size: Option<(u32, u32)>,
    ) -> TextureOptions {
        self.texture_processing(path, category, frame_size, None)
            .options
    }

    /// Разрешает Auto и ручные правила одним проходом; признаки относятся к исходным кадрам.
    #[must_use]
    pub fn texture_processing(
        &self,
        path: &str,
        category: Category,
        frame_size: Option<(u32, u32)>,
        features: Option<&ImageFeatures>,
    ) -> TextureProcessing {
        self.texture_policy(path, category, frame_size)
            .processing(features)
    }

    pub(crate) fn texture_policy(
        &self,
        path: &str,
        category: Category,
        frame_size: Option<(u32, u32)>,
    ) -> TexturePolicy {
        let mut options = self.textures;
        let mut scale = self.scale;
        if self.reproduction {
            options.premultiplied_alpha = false;
            options.independent_frames = false;
        }
        let mut automatic = AutoPolicy {
            algorithm: self.auto_algorithm,
            sharpening: self.auto_sharpening,
        };
        for rule in self.rules(path, category) {
            automatic.algorithm &= rule.algorithm.is_none();
            automatic.sharpening &= rule.sharpening.is_none();
            if let Some(value) = rule.scale {
                scale = value;
            }
            rule.apply(&mut options);
        }
        let selection = if automatic.algorithm {
            auto_algorithm::Selection::new(path, category, frame_size, scale)
        } else {
            auto_algorithm::Selection::Fixed(None)
        };
        TexturePolicy {
            scale,
            automatic,
            options,
            selection,
        }
    }

    #[must_use]
    pub fn needs_pixel_analysis(
        &self,
        path: &str,
        category: Category,
        frame_size: Option<(u32, u32)>,
    ) -> bool {
        self.texture_policy(path, category, frame_size)
            .needs_pixel_analysis()
    }

    /// Возвращает автоматическое назначение, если его не заменяет явное правило.
    #[must_use]
    pub fn automatic_algorithm(
        &self,
        path: &str,
        category: Category,
        frame_size: Option<(u32, u32)>,
    ) -> Option<AlgorithmRecommendation> {
        self.texture_processing(path, category, frame_size, None)
            .recommendation
    }
}
impl Default for Profile {
    fn default() -> Self {
        Self {
            name: "Stellaris UI Scaler".into(),
            mod_version: "0.1.0".into(),
            supported_version: None,
            picture: None,
            source_mods: Vec::new(),
            auto_source_mods: None,
            disabled_source_mods: BTreeSet::new(),
            resolution: [3840, 2160],
            scale: Scale::default(),
            font_scale: None,
            font_mode: FontMode::default(),
            fonts: BTreeMap::new(),
            reproduction: false,
            auto_algorithm: false,
            auto_sharpening: false,
            textures: TextureOptions::default(),
            outliner: OutlinerOptions::default(),
            categories: BTreeMap::new(),
            directories: BTreeMap::new(),
            assets: BTreeMap::new(),
        }
    }
}
