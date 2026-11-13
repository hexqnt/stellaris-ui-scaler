use std::{
    borrow::Cow,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};

use crate::{
    assets, compiler,
    pdx::{Document, Node, unescape},
};

/// Возвращает имя из descriptor.mod, а при его отсутствии — имя каталога.
#[must_use]
pub fn mod_name(root: &Path) -> String {
    if let Ok(source) = fs::read_to_string(root.join("descriptor.mod"))
        && let Ok(document) = Document::parse(&source)
        && let Some(name) = document
            .nodes
            .iter()
            .find(|node| node.key == Some("name"))
            .and_then(Node::atom)
            .filter(|name| !name.trim().is_empty())
    {
        return unescape(name).into_owned();
    }
    root.file_name()
        .unwrap_or(root.as_os_str())
        .to_string_lossy()
        .into_owned()
}

pub(super) fn source_root(source: &str, user_data: &Path) -> Result<Option<PathBuf>> {
    let document = Document::parse(source)?;
    ensure!(
        !document
            .nodes
            .iter()
            .any(|node| node.key == Some("archive")),
        "Archived source mods are not supported"
    );
    let mut paths = document
        .nodes
        .iter()
        .filter(|node| node.key == Some("path"));
    let path = paths
        .next()
        .and_then(Node::atom)
        .context("Mod descriptor has no path")?;
    ensure!(paths.next().is_none(), "Mod descriptor has multiple paths");
    ensure!(!path.is_empty(), "Mod descriptor has an empty path");
    let path = unescape(path);
    let path = if path.contains('\\') {
        Cow::Owned(path.replace('\\', "/"))
    } else {
        path
    };
    let path = Path::new(path.as_ref());
    let root = if path.is_absolute() {
        Cow::Borrowed(path)
    } else {
        Cow::Owned(user_data.join(path))
    };
    let root = root
        .canonicalize()
        .with_context(|| format!("Cannot locate enabled mod: {}", root.display()))?;
    ensure!(
        root.is_dir(),
        "Enabled mod path is not a directory: {}",
        root.display()
    );
    if root.join(compiler::manifest_name()).is_file() {
        return Ok(None);
    }
    check_replace_paths(&document)?;
    let metadata_path = root.join("descriptor.mod");
    match fs::read_to_string(&metadata_path) {
        Ok(source) => {
            let metadata = Document::parse(&source)
                .with_context(|| format!("Invalid mod metadata: {}", metadata_path.display()))?;
            check_replace_paths(&metadata)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error)
                .with_context(|| format!("Cannot read mod metadata: {}", metadata_path.display()));
        }
    }
    Ok(assets::has_ui_resources(&root).then_some(root))
}

fn check_replace_paths(document: &Document<'_>) -> Result<()> {
    for node in document
        .nodes
        .iter()
        .filter(|node| node.key == Some("replace_path"))
    {
        let path = node.atom().context("replace_path must be a string")?;
        let path = assets::normalize_reference(path);
        let path = path.trim_end_matches('/');
        assets::safe_relative(path)?;
        ensure!(
            !assets::RESOURCE_DIRECTORIES.iter().any(|dir| {
                path == *dir
                    || path
                        .strip_prefix(dir)
                        .is_some_and(|rest| rest.starts_with('/'))
            }),
            "replace_path affecting UI resources is not supported: {path}"
        );
    }
    Ok(())
}
