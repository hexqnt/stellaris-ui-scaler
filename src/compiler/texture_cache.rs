use std::{fs, io::Write, path::PathBuf};

use anyhow::{Context, Result, ensure};

use crate::{
    assets::Installation,
    atomic_file::AtomicFile,
    config::{Profile, TexturePolicy},
    textures,
};

use super::{Control, PIPELINE_VERSION, TextureAsset, png, preview::render_texture};

pub(super) struct TextureCache {
    root: PathBuf,
    key: Vec<u8>,
}

impl TextureCache {
    pub(super) fn open(root: PathBuf) -> Result<Self> {
        ensure!(!root.is_symlink(), "Cache must not be a symbolic link");
        fs::create_dir_all(&root)?;
        Ok(Self {
            root,
            key: Vec::with_capacity(256),
        })
    }
    pub(super) fn prepare<'a>(
        &mut self,
        game: &Installation,
        asset: &'a TextureAsset,
        profile: &Profile,
    ) -> Result<PendingTexture<'a>> {
        let source = game.read(&asset.path)?;
        let policy = asset.policy(profile);
        let options = policy.processing(None).options;
        let scale = policy.scale;

        let mut hash = blake3::Hasher::new();
        hash.update(PIPELINE_VERSION.as_bytes());
        hash.update(&source);
        // Буфер переиспользуется: множество коротких записей прямо в BLAKE3 медленнее одной.
        self.key.clear();
        serde_json::to_writer(
            &mut self.key,
            &(
                scale,
                &options,
                policy.automatic,
                asset.category,
                asset.frames,
                asset.tiled,
                profile.reproduction,
                &asset.path,
                textures::cache_identity(options.algorithm),
            ),
        )?;
        hash.update(&self.key);
        let dimensions = asset.target_dimensions(scale)?;
        let target = self.root.join(format!("{}.png", hash.finalize().to_hex()));
        let cached = match fs::read(&target) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        Ok(PendingTexture {
            asset,
            source,
            cached,
            target,
            dimensions,
            policy,
        })
    }
}

pub(super) struct PendingTexture<'a> {
    asset: &'a TextureAsset,
    source: Vec<u8>,
    cached: Option<Vec<u8>>,
    target: PathBuf,
    dimensions: (u32, u32),
    policy: TexturePolicy,
}

impl PendingTexture<'_> {
    pub(super) fn uses_neural(&self) -> bool {
        self.policy.uses_neural()
    }

    pub(super) fn process(
        self,
        processor: &textures::Processor,
        control: &Control,
    ) -> Result<ProcessedTexture> {
        let cached_image = self
            .cached
            .as_deref()
            .and_then(|bytes| textures::decode(bytes).ok())
            .filter(|image| image.dimensions() == self.dimensions);
        let (bytes, image, hit) = if let (Some(bytes), Some(image)) = (self.cached, cached_image) {
            (bytes, image, true)
        } else {
            let image = textures::decode(&self.source)
                .with_context(|| format!("Cannot decode {}", self.asset.path))?;
            let features = self.policy.analyze(&image, self.asset.frames)?;
            let options = self.policy.processing(features.as_ref()).options;
            let image = render_texture(
                self.asset,
                self.policy.scale,
                &image,
                &options,
                processor,
                control,
            )?;
            (png(&image)?, image, false)
        };
        // Для ресурсных значков используем уже декодированное изображение, минуя повторное чтение PNG.
        let resource_dds = textures::is_resource_icon(&self.asset.path)
            .then(|| textures::resource_dds(&image))
            .transpose()?;
        Ok(ProcessedTexture {
            png: bytes,
            resource_dds,
            hit,
            target: self.target,
        })
    }
}

pub(super) struct ProcessedTexture {
    pub(super) png: Vec<u8>,
    pub(super) resource_dds: Option<Vec<u8>>,
    pub(super) hit: bool,
    target: PathBuf,
}

impl ProcessedTexture {
    pub(super) fn store(&self) -> Result<()> {
        if !self.hit {
            AtomicFile::prepare(&self.target, |file| file.write_all(&self.png))?.publish()?;
        }
        Ok(())
    }
}
