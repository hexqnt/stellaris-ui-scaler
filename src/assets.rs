use std::{
    borrow::Cow,
    collections::BTreeMap,
    fs,
    io::{BufReader, Read},
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex},
};

use anyhow::{Context, Result, ensure};
use serde::Serialize;

const UI_DIRECTORIES: [&str; 3] = ["interface", "gfx", "fonts"];

pub(crate) const RESOURCE_DIRECTORIES: &[&str] = &[
    UI_DIRECTORIES[0],
    UI_DIRECTORIES[1],
    UI_DIRECTORIES[2],
    "unchecked_defines",
    "localisation",
];

type SharedArchive = Arc<Mutex<zip::ZipArchive<BufReader<fs::File>>>>;

#[derive(Debug, Clone)]
pub enum Source {
    File(PathBuf),
    Archive {
        archive: SharedArchive,
        member: String,
    },
}
#[derive(Debug, Clone)]
pub struct Installation {
    pub root: PathBuf,
    pub source_mods: Vec<PathBuf>,
    pub version: String,
    pub resources: BTreeMap<String, Source>,
    pub archives: usize,
}

impl Installation {
    /// # Errors
    /// Возвращает ошибку установки, архива или доступа к файлам.
    pub fn open(path: &Path) -> Result<Self> {
        let root = crate::paths::game_resources(path)
            .canonicalize()
            .context("Cannot locate game resources")?;
        ensure!(
            root.join("interface").is_dir() && root.join("gfx").is_dir(),
            "Not a Stellaris installation: {}",
            root.display()
        );
        let version = fs::read_to_string(root.join("launcher-settings.json"))
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .and_then(|v| {
                v.get("rawVersion")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| "unknown".into());
        let mut resources = BTreeMap::new();
        index_directory(&root, &mut resources)?;
        let mut archives = 0;
        for path in files(&root.join("dlc"))? {
            if path.extension().is_none_or(|e| e != "zip") {
                continue;
            }
            let zip = zip::ZipArchive::new(BufReader::new(fs::File::open(&path)?))
                .with_context(|| format!("Cannot index {}", path.display()))?;
            let metadata = zip.metadata();
            let archive = Arc::new(Mutex::new(zip));
            archives += 1;
            for i in 0..metadata.len() {
                let member = metadata.entry(i)?;
                let name = member.name()?;
                let key = normalize_reference(&name);
                if member.is_dir() || safe_relative(&key).is_err() {
                    continue;
                }
                if key
                    .split_once('/')
                    .is_some_and(|(directory, _)| UI_DIRECTORIES.contains(&directory))
                {
                    resources
                        .entry(key.into_owned())
                        .or_insert_with(|| Source::Archive {
                            archive: Arc::clone(&archive),
                            member: name.into_owned(),
                        });
                }
            }
        }
        Ok(Self {
            root,
            source_mods: Vec::new(),
            version,
            resources,
            archives,
        })
    }

    /// Накладывает ресурсы распакованных UI-модов по порядку: последний файл имеет приоритет.
    /// # Errors
    /// Возвращает ошибку каталога или чтения ресурсов.
    pub fn with_source_mods(mut self, paths: &[PathBuf]) -> Result<Self> {
        for path in paths {
            let root = source_mod_root(path)?;
            ensure!(
                has_ui_resources(&root),
                "Source mod must be an unpacked directory with UI resources: {}",
                root.display()
            );
            index_directory(&root, &mut self.resources)?;
            self.source_mods.push(root);
        }
        Ok(self)
    }

    pub(crate) fn check_source_mods(&self, requested: &[PathBuf]) -> Result<()> {
        const CHANGED: &str = "Source mods changed; scan assets again before building";
        ensure!(requested.len() == self.source_mods.len(), CHANGED);
        for (path, scanned) in requested.iter().zip(&self.source_mods) {
            ensure!(source_mod_root(path)? == *scanned, CHANGED);
        }
        Ok(())
    }
    /// # Errors
    /// Возвращает ошибку отсутствующего ресурса, архива или небезопасного пути.
    pub fn read(&self, relative: &str) -> Result<Vec<u8>> {
        safe_relative(relative)?;
        let source = self
            .resources
            .get(lowercase(relative).as_ref())
            .with_context(|| format!("Missing game resource: {relative}"))?;
        match source {
            Source::File(path) => fs::read(path).with_context(|| format!("Cannot read {relative}")),
            Source::Archive { archive, member } => {
                // Один каталог ZIP и файловый буфер переиспользуются всеми ресурсами архива.
                let mut zip = archive
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Archive lock poisoned"))?;
                let mut entry = zip.by_name(member)?;
                ensure!(
                    entry.size() <= 256 * 1024 * 1024,
                    "Archive entry too large: {member}"
                );
                let mut bytes = Vec::with_capacity(usize::try_from(entry.size())?);
                entry.read_to_end(&mut bytes)?;
                drop(entry);
                drop(zip);
                Ok(bytes)
            }
        }
    }
    /// # Errors
    /// Возвращает ошибку чтения или некорректного UTF-8.
    pub fn text(&self, relative: &str) -> Result<String> {
        let bytes = self.read(relative)?;
        let mut text =
            String::from_utf8(bytes).with_context(|| format!("Invalid UTF-8: {relative}"))?;
        let start = text.len() - text.trim_start_matches('\u{feff}').len();
        if start > 0 {
            text.drain(..start);
        }
        Ok(text)
    }
    #[allow(
        clippy::case_sensitive_file_extension_comparisons,
        reason = "Индекс нормализован в нижний регистр"
    )]
    pub fn interface_files(&self) -> impl Iterator<Item = &str> {
        self.resources
            .keys()
            .filter(|p| p.starts_with("interface/") && (p.ends_with(".gui") || p.ends_with(".gfx")))
            .map(String::as_str)
    }
    #[must_use]
    pub fn contains(&self, path: &str) -> bool {
        self.resources.contains_key(lowercase(path).as_ref())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Diagnostic {
    pub path: String,
    pub span: Option<std::ops::Range<usize>>,
    pub rule: String,
    pub message: String,
}

fn source_mod_root(path: &Path) -> Result<PathBuf> {
    path.canonicalize()
        .with_context(|| format!("Cannot locate source mod: {}", path.display()))
}

pub(crate) fn has_ui_resources(root: &Path) -> bool {
    UI_DIRECTORIES.iter().any(|dir| root.join(dir).is_dir())
}

fn index_directory(root: &Path, resources: &mut BTreeMap<String, Source>) -> Result<()> {
    for dir in RESOURCE_DIRECTORIES {
        for file in files(&root.join(dir))? {
            let relative = crate::paths::portable(file.strip_prefix(root)?)?;
            resources.insert(
                normalize_reference(&relative).into_owned(),
                Source::File(file),
            );
        }
    }
    Ok(())
}

/// # Errors
/// Отклоняет абсолютные пути и выход за корень ресурсов.
pub fn safe_relative(path: &str) -> Result<&Path> {
    let relative = Path::new(path);
    ensure!(
        !path.is_empty()
            && relative
                .components()
                .all(|c| matches!(c, Component::Normal(_)))
            && !path.contains(['\\', ':']),
        "Unsafe resource path: {path}"
    );
    Ok(relative)
}

fn lowercase(path: &str) -> Cow<'_, str> {
    if path
        .bytes()
        .all(|byte| byte.is_ascii() && !byte.is_ascii_uppercase())
    {
        Cow::Borrowed(path)
    } else {
        Cow::Owned(path.to_lowercase())
    }
}

pub(crate) fn normalize_reference(path: &str) -> Cow<'_, str> {
    if path.contains('\\') {
        let mut normalized = path.replace('\\', "/");
        if normalized.is_ascii() {
            normalized.make_ascii_lowercase();
            Cow::Owned(normalized)
        } else {
            Cow::Owned(normalized.to_lowercase())
        }
    } else {
        lowercase(path)
    }
}

/// # Errors
/// Возвращает ошибку чтения каталогов.
pub fn files(root: &Path) -> Result<Vec<PathBuf>> {
    fn walk(dir: &Path, result: &mut Vec<PathBuf>) -> Result<()> {
        for entry in fs::read_dir(dir)
            .with_context(|| format!("Cannot read resource directory: {}", dir.display()))?
        {
            let entry = entry?;
            let kind = entry.file_type()?;
            ensure!(
                !kind.is_symlink(),
                "Symbolic links are not supported in resource and output trees: {}",
                entry.path().display()
            );
            if kind.is_dir() {
                walk(&entry.path(), result)?;
            } else if kind.is_file() {
                result.push(entry.path());
            }
        }
        Ok(())
    }
    let mut result = Vec::new();
    let metadata = match fs::symlink_metadata(root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(result),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("Cannot inspect resource directory: {}", root.display()));
        }
    };
    ensure!(
        !metadata.is_symlink(),
        "Symbolic links are not supported in resource and output trees: {}",
        root.display()
    );
    walk(root, &mut result)?;
    result.sort();
    Ok(result)
}

#[derive(Clone, Copy)]
pub(crate) enum TextureRole {
    Color,
    Mask,
}

pub(crate) fn texture_reference<'a>(
    node: &crate::pdx::Node<'a>,
) -> Option<(Cow<'a, str>, TextureRole)> {
    let key = node.key?;
    let role = if ["animationmaskfile", "masking_texture"]
        .iter()
        .any(|name| key.eq_ignore_ascii_case(name))
    {
        TextureRole::Mask
    } else if [
        "texturefile",
        "texturefile1",
        "texturefile2",
        "animationtexturefile",
    ]
    .iter()
    .any(|name| key.eq_ignore_ascii_case(name))
    {
        TextureRole::Color
    } else {
        return None;
    };
    Some((normalize_reference(node.atom()?), role))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_references_preserve_unicode_case_folding_and_borrow_ascii() {
        let path = "gfx/interface/icon.dds";
        std::assert_matches!(normalize_reference(path), Cow::Borrowed(value) if value == path);
        for path in [
            r"GFX\Interface\Icon.DDS",
            "GFX/ΣΟΣ/ФОН.DDS",
            r"GFX\ΣΟΣ\ФОН.DDS",
        ] {
            assert_eq!(
                normalize_reference(path),
                path.replace('\\', "/").to_lowercase()
            );
        }
    }
}
