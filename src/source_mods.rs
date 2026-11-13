mod descriptor;
mod order;

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::{
    assets,
    config::{AutoSourceMods, Profile},
    user_data,
};

pub use descriptor::mod_name;
pub use order::ModListMismatch;

/// Снимок модов из экспортированной лаунчером конфигурации запуска.
#[derive(Debug, Serialize)]
pub struct Discovery {
    pub user_data: PathBuf,
    pub source_mods: Vec<PathBuf>,
    pub skipped_mods: usize,
}

#[derive(Deserialize)]
struct LaunchConfig {
    enabled_mods: Vec<String>,
}

/// Обновляет сохранённый список только после успешного обнаружения всех источников.
/// # Errors
/// Возвращает ошибку обнаружения; прежний список остаётся неизменным.
pub fn refresh(game: &Path, settings: &mut AutoSourceMods) -> Result<Discovery> {
    let discovered = discover(game, settings.user_data_dir.as_deref())?;
    settings.source_mods.clone_from(&discovered.source_mods);
    Ok(discovered)
}

/// Сверяет сохранённый порядок с текущим экспортом и добавляет ручные переопределения.
/// # Errors
/// Возвращает ошибку профиля, обнаружения или расхождения списков модов.
pub fn resolve(game: &Path, profile: &Profile) -> Result<Vec<PathBuf>> {
    profile.check()?;
    let mut resolved = match &profile.auto_source_mods {
        Some(settings) => {
            let discovered = discover(game, settings.user_data_dir.as_deref())?;
            if let Some(changes) =
                ModListMismatch::between(&settings.source_mods, &discovered.source_mods)
            {
                return Err(changes.into());
            }
            discovered.source_mods
        }
        None => Vec::new(),
    };
    resolved.retain(|path| !profile.disabled_source_mods.contains(path));
    resolved.extend(
        profile
            .source_mods
            .iter()
            .filter(|path| !profile.disabled_source_mods.contains(*path))
            .cloned(),
    );
    Ok(resolved)
}

/// Читает список запуска, сохраняя порядок файловых переопределений.
/// # Errors
/// Возвращает ошибку чтения или неподдерживаемого активного мода; частичный список не возвращается.
pub fn discover(game: &Path, directory: Option<&Path>) -> Result<Discovery> {
    let directory =
        directory.map_or_else(|| user_data::directory(game), |path| Ok(path.to_owned()))?;
    let directory = directory
        .canonicalize()
        .with_context(|| format!("Cannot locate user data directory: {}", directory.display()))?;
    let config_path = directory.join("dlc_load.json");
    let config: LaunchConfig = serde_json::from_slice(
        &fs::read(&config_path)
            .with_context(|| format!("Cannot read {}", config_path.display()))?,
    )
    .with_context(|| format!("Invalid launch configuration: {}", config_path.display()))?;
    let mut discovery = Discovery {
        user_data: directory,
        source_mods: Vec::new(),
        skipped_mods: 0,
    };
    for relative in config.enabled_mods {
        let relative = relative.replace('\\', "/");
        let descriptor = discovery.user_data.join(assets::safe_relative(&relative)?);
        let source = fs::read_to_string(&descriptor).with_context(|| {
            format!(
                "Cannot read enabled mod descriptor: {}",
                descriptor.display()
            )
        })?;
        let root = descriptor::source_root(&source, &discovery.user_data)
            .with_context(|| format!("Unsupported enabled mod: {}", descriptor.display()))?;
        if let Some(root) = root {
            // Повторное упоминание каталога сохраняет его последний приоритет.
            discovery.source_mods.retain(|previous| *previous != root);
            discovery.source_mods.push(root);
        } else {
            discovery.skipped_mods += 1;
        }
    }
    Ok(discovery)
}
