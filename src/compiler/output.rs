use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};

use crate::{
    assets::{self, Installation},
    atomic_file::{AtomicFile, Durability},
    config::Profile,
    paths,
};

use super::{MANIFEST, Manifest};

struct StagingDirectory(PathBuf);
impl Drop for StagingDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub(super) struct OutputTransaction {
    destination: PathBuf,
    previous: Option<Manifest>,
    stage: StagingDirectory,
    path_line: String,
}

impl OutputTransaction {
    pub(super) fn prepare(output: &Path, game: &Installation) -> Result<Self> {
        ensure!(!output.is_symlink(), "Output must not be a symbolic link");
        let parent = output
            .parent()
            .filter(|p| !p.is_empty())
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;
        let parent = parent.canonicalize()?;
        let name = output
            .file_name()
            .context("Output must name a mod directory")?;
        let output = parent.join(name);
        let path_line = paths::launcher_line(&output)?;
        for source in std::iter::once(&game.root).chain(&game.source_mods) {
            ensure!(
                !output.starts_with(source) && !source.starts_with(&output),
                "Output must be separate from source directories: {}",
                source.display()
            );
        }
        let previous: Option<Manifest> = if output.exists() {
            ensure!(
                output.join(MANIFEST).is_file(),
                "Existing output has no ownership manifest"
            );
            let manifest: Manifest = serde_json::from_slice(&fs::read(output.join(MANIFEST))?)?;
            ensure!(manifest.format == 1, "Unsupported manifest format");
            for path in manifest.files.keys() {
                assets::safe_relative(path)?;
            }
            Some(manifest)
        } else {
            None
        };
        let stage_path = parent.join(format!(".ui-scaler-stage-{}", std::process::id()));
        ensure!(
            !stage_path.exists(),
            "Another build is active or a stale staging directory exists: {}",
            stage_path.display()
        );
        fs::create_dir(&stage_path)?;
        let stage = StagingDirectory(stage_path);
        Ok(Self {
            destination: output,
            previous,
            stage,
            path_line,
        })
    }
    pub(super) fn root(&self) -> &Path {
        &self.stage.0
    }
    pub(super) fn cache_root(&self) -> PathBuf {
        self.destination.with_file_name(".ui-scaler-cache")
    }

    pub(super) fn record_files(&self, profile: &Profile, game: &Installation) -> Result<()> {
        let mut managed = BTreeMap::new();
        for file in assets::files(self.root())? {
            let relative = paths::portable(file.strip_prefix(self.root())?)?.into_owned();
            let mut hash = blake3::Hasher::new();
            hash.update_reader(fs::File::open(file)?)?;
            managed.insert(relative, hash.finalize().to_hex().to_string());
        }
        let manifest = Manifest {
            format: 1,
            game_version: game.version.clone(),
            profile: profile.clone(),
            source_mods: game.source_mods.clone(),
            files: managed,
        };
        write(
            self.root(),
            MANIFEST,
            &serde_json::to_vec_pretty(&manifest)?,
        )?;
        if let Some(previous) = &self.previous {
            for file in assets::files(&self.destination)? {
                let relative = paths::portable(file.strip_prefix(&self.destination)?)?;
                if relative == MANIFEST || previous.files.contains_key(relative.as_ref()) {
                    continue;
                }
                ensure!(
                    !manifest.files.contains_key(relative.as_ref()),
                    "New generated file conflicts with user file: {relative}"
                );
                let target = prepare_target(self.root(), &relative)?;
                fs::copy(&file, &target)
                    .with_context(|| format!("Cannot preserve user file: {}", file.display()))?;
            }
        }
        Ok(())
    }
    pub(super) fn publish(self, descriptor: &str) -> Result<PathBuf> {
        let backup = self
            .destination
            .with_file_name(format!(".ui-scaler-backup-{}", std::process::id()));
        ensure!(!backup.exists(), "Backup directory already exists");
        let launcher = self.destination.with_added_extension("mod");
        let launcher_text = format!("{descriptor}{}\n", self.path_line);
        ensure!(
            !launcher.is_symlink(),
            "Launcher file must not be a symbolic link"
        );
        if launcher.exists() {
            let old = fs::read_to_string(&launcher)?;
            ensure!(
                old.lines().any(|line| line == self.path_line),
                "Launcher descriptor {} does not point to output directory {}; choose another output directory or remove the descriptor if it is obsolete",
                launcher.display(),
                self.destination.display()
            );
        }
        let launcher_file =
            AtomicFile::prepare(&launcher, launcher_text.as_bytes(), Durability::Buffered)
                .context("Cannot prepare launcher descriptor")?;
        let existed = self.destination.exists();
        if existed {
            fs::rename(&self.destination, &backup)?;
        }
        let publish = (|| -> Result<()> {
            fs::rename(self.root(), &self.destination)?;
            launcher_file.publish()?;
            Ok(())
        })();
        if let Err(error) = publish {
            if self.destination.exists() {
                fs::remove_dir_all(&self.destination)?;
            }
            if existed {
                fs::rename(&backup, &self.destination)?;
            }
            return Err(error);
        }
        if existed {
            fs::remove_dir_all(backup)?;
        }
        Ok(self.destination)
    }
}

pub(super) fn write(root: &Path, relative: &str, bytes: &[u8]) -> Result<()> {
    let target = prepare_target(root, relative)?;
    fs::write(&target, bytes).with_context(|| format!("Cannot write {}", target.display()))
}

fn prepare_target(root: &Path, relative: &str) -> Result<PathBuf> {
    let target = root.join(assets::safe_relative(relative)?);
    fs::create_dir_all(target.parent().context("Missing parent directory")?)?;
    Ok(target)
}
