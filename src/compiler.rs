#![allow(
    clippy::case_sensitive_file_extension_comparisons,
    reason = "Пути индекса нормализованы в нижний регистр"
)]

mod analysis;
mod interface;
mod output;
mod preview;
mod texture_cache;
mod texture_pipeline;

use std::{
    collections::{BTreeMap, BTreeSet},
    io::Cursor,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use crate::{
    assets::{Diagnostic, Installation},
    config::{Category, ImageFeatures, Profile, Scale, TexturePolicy},
    source_mods, textures, validation,
};

pub use analysis::{plan, plan_with_mods, plan_with_profile, scan_enabled_mods};
pub use preview::{
    PreviewEvent, PreviewSession, TexturePreview, preview, preview_candidates,
    preview_candidates_with_analysis,
};

const MANIFEST: &str = "ui-scaler-manifest.json";
const PIPELINE_VERSION: &str = "5";
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressStage {
    Scan,
    Textures,
    Interface,
    Complete,
}

impl ProgressStage {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Scan => "scan",
            Self::Textures => "textures",
            Self::Interface => "interface",
            Self::Complete => "complete",
        }
    }
}
impl std::fmt::Display for ProgressStage {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Источник размера спрайта определяет, нужен ли дополнительный scale виджета.
pub enum SpriteSizing {
    Texture,
    Geometry,
    RenderTarget,
}

#[derive(Clone, Default)]
pub struct Control {
    cancelled: Arc<AtomicBool>,
}
impl Control {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
    /// # Errors
    /// Возвращает ошибку при запросе отмены.
    pub fn check(&self) -> Result<()> {
        ensure!(
            !self.cancelled.load(Ordering::Relaxed),
            "Operation cancelled"
        );
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct Progress {
    pub stage: ProgressStage,
    pub completed: usize,
    pub total: usize,
}
impl Progress {
    const fn new(stage: ProgressStage, completed: usize, total: usize) -> Self {
        Self {
            stage,
            completed,
            total,
        }
    }
}

impl std::fmt::Display for Progress {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{}: {}/{}",
            self.stage, self.completed, self.total
        )
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TextureAsset {
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub frames: u32,
    pub category: Category,
    pub tiled: bool,
}
impl TextureAsset {
    /// Предварительные параметры без чтения пикселей; окончательный выбор использует `processing` с признаками исходника.
    #[must_use]
    pub fn options(&self, profile: &Profile) -> crate::config::TextureOptions {
        self.processing(profile, None).options
    }

    #[must_use]
    pub fn processing(
        &self,
        profile: &Profile,
        features: Option<&crate::config::ImageFeatures>,
    ) -> crate::config::TextureProcessing {
        self.policy(profile).processing(features)
    }

    pub(crate) fn policy(&self, profile: &Profile) -> crate::config::TexturePolicy {
        profile.texture_policy(&self.path, self.category, self.frame_size())
    }

    /// # Errors
    /// Возвращает ошибку раскладки кадров при необходимом анализе исходника.
    pub fn analyze(
        &self,
        profile: &Profile,
        source: &image::RgbaImage,
    ) -> Result<Option<crate::config::ImageFeatures>> {
        self.policy(profile).analyze(source, self.frames)
    }

    #[must_use]
    pub fn frame_size(&self) -> Option<(u32, u32)> {
        self.width
            .checked_div(self.frames)
            .map(|width| (width, self.height))
    }

    fn render(
        &self,
        source: &image::RgbaImage,
        policy: TexturePolicy,
        features: Option<&ImageFeatures>,
        processor: &textures::Processor,
        control: &Control,
    ) -> Result<image::RgbaImage> {
        ensure!(
            source.dimensions() == (self.width, self.height),
            "Source dimensions changed since scan: {}",
            self.path
        );
        let boundary = if self.tiled {
            textures::Boundary::Periodic
        } else {
            textures::Boundary::Image
        };
        processor.process(
            source,
            policy.scale,
            self.frames,
            &policy.processing(features).options,
            boundary,
            &|| control.check(),
        )
    }

    fn target_dimensions(&self, scale: Scale) -> Result<(u32, u32)> {
        textures::target_dimensions(self.width, self.height, self.frames, scale)
    }
}

#[derive(Debug, Clone)]
pub struct Sprite {
    pub file: String,
    pub name: String,
    pub span: std::ops::Range<usize>,
    pub references: Vec<String>,
    pub sprite_sheet: Option<String>,
    pub tiled: bool,
    pub convert: bool,
    pub sizing: SpriteSizing,
}
#[derive(Debug, Clone)]
pub struct BuildPlan {
    pub fonts: crate::fonts::Catalog,
    pub game: Installation,
    pub sources: BTreeMap<String, String>,
    pub sprites: Vec<Sprite>,
    pub textures: Vec<TextureAsset>,
    pub diagnostics: Vec<Diagnostic>,
}

impl BuildPlan {
    #[must_use]
    pub fn report(&self) -> ScanReport<'_> {
        ScanReport {
            version: &self.game.version,
            source_mods: &self.game.source_mods,
            interface_files: self.sources.len(),
            archives: self.game.archives,
            textures: &self.textures,
            diagnostics: &self.diagnostics,
        }
    }
}

/// Результат сканирования текущих включённых модов вместе с новым снимком источников.
#[derive(Debug)]
pub struct EnabledModScan {
    pub plan: BuildPlan,
    pub settings: crate::config::AutoSourceMods,
    pub disabled_source_mods: BTreeSet<PathBuf>,
    pub changes: Option<source_mods::ModListMismatch>,
    pub removed_manual_sources: bool,
}
#[derive(Debug, Clone, Serialize)]
pub struct ScanReport<'a> {
    pub version: &'a str,
    pub source_mods: &'a [PathBuf],
    pub interface_files: usize,
    pub archives: usize,
    pub textures: &'a [TextureAsset],
    pub diagnostics: &'a [Diagnostic],
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub format: u32,
    pub game_version: String,
    pub profile: Profile,
    #[serde(default)]
    pub source_mods: Vec<PathBuf>,
    pub files: BTreeMap<String, String>,
}
#[derive(Debug, Serialize)]
pub struct BuildReport {
    pub output: PathBuf,
    pub textures: usize,
    pub cache_hits: usize,
    pub diagnostics: Vec<Diagnostic>,
    pub validation: validation::ValidationReport,
}

fn png(image: &image::RgbaImage) -> Result<Vec<u8>> {
    let mut result = Cursor::new(Vec::new());
    image.write_to(&mut result, image::ImageFormat::Png)?;
    Ok(result.into_inner())
}

fn mod_picture(path: &Path) -> Result<Vec<u8>> {
    let image = image::ImageReader::open(path)
        .with_context(|| format!("Cannot open mod picture: {}", path.display()))?
        .with_guessed_format()?
        .decode()
        .with_context(|| format!("Cannot decode mod picture: {}", path.display()))?;
    png(&image.into_rgba8())
}

/// Политика повторного использования результатов обработки текстур.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CacheMode {
    #[default]
    Reuse,
    /// Пересчитать все текстуры и обновить кеш.
    Rebuild,
}

/// # Errors
/// Возвращает ошибку преобразования, проверки, публикации или отмены; предыдущая сборка сохраняется.
pub fn build(
    plan: &BuildPlan,
    profile: &Profile,
    output: &Path,
    control: &Control,
    progress: &mut dyn FnMut(Progress),
) -> Result<BuildReport> {
    build_with_cache(plan, profile, output, CacheMode::Reuse, control, progress)
}

/// Генерирует мод с выбранной политикой кеша.
///
/// # Errors
/// Возвращает ошибку преобразования, проверки, публикации или отмены; предыдущая сборка сохраняется.
pub fn build_with_cache(
    plan: &BuildPlan,
    profile: &Profile,
    output: &Path,
    cache_mode: CacheMode,
    control: &Control,
    progress: &mut dyn FnMut(Progress),
) -> Result<BuildReport> {
    control.check()?;
    let sources = source_mods::resolve(&plan.game.root, profile)?;
    plan.game.check_source_mods(&sources)?;
    plan.fonts.check_rules(&profile.fonts)?;
    control.check()?;
    let picture = profile.picture.as_deref().map(mod_picture).transpose()?;
    let mut texture_outputs = BTreeSet::new();
    for asset in &plan.textures {
        ensure!(
            texture_outputs.insert(textures::output_name(&asset.path)),
            "Output texture name collision: {}",
            asset.path
        );
    }
    drop(texture_outputs);
    let transaction = output::OutputTransaction::prepare(output, &plan.game)?;
    let mut cache = texture_cache::TextureCache::open(transaction.cache_root(), cache_mode)?;
    let mut diagnostics = plan.diagnostics.clone();
    let cache_hits = texture_pipeline::compile(
        plan,
        profile,
        transaction.root(),
        &mut cache,
        control,
        progress,
    )?;
    let converted = plan
        .textures
        .iter()
        .map(|asset| asset.path.as_str())
        .collect();
    interface::compile(
        plan,
        profile,
        transaction.root(),
        &converted,
        control,
        progress,
        &mut diagnostics,
    )?;
    let mut descriptor = format!(
        "name=\"{}\"\nversion=\"{}\"\ntags={{ \"UI\" \"Graphics\" }}\nsupported_version=\"{}\"\n",
        profile.name,
        profile.mod_version,
        profile
            .supported_version
            .as_deref()
            .unwrap_or(&plan.game.version)
    );
    if let Some(picture) = picture {
        output::write(transaction.root(), "thumbnail.png", &picture)?;
        descriptor.push_str("picture=\"thumbnail.png\"\n");
    }
    output::write(transaction.root(), "descriptor.mod", descriptor.as_bytes())?;
    transaction.record_files(profile, &plan.game)?;
    let validation = validation::validate(&plan.game, transaction.root())?;
    ensure!(
        validation.errors.is_empty(),
        "Generated mod validation failed: {}",
        serde_json::to_string(&validation.errors)?
    );
    control.check()?;
    let report = BuildReport {
        output: transaction.publish(&descriptor)?,
        textures: converted.len(),
        cache_hits,
        diagnostics,
        validation,
    };
    progress(Progress::new(ProgressStage::Complete, 1, 1));
    Ok(report)
}

#[must_use]
pub const fn manifest_name() -> &'static str {
    MANIFEST
}
