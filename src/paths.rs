use std::{borrow::Cow, path::Path};

use anyhow::{Context, Result, ensure};

/// Поддерживает корень установки и каталог ресурсов внутри GOG-установки.
pub fn game_resources(path: &Path) -> Cow<'_, Path> {
    if path.join("interface").is_dir() {
        Cow::Borrowed(path)
    } else {
        Cow::Owned(path.join("game"))
    }
}

/// Сохраняет регистр и представляет нативный путь с разделителями игры.
pub fn portable(path: &Path) -> Result<Cow<'_, str>> {
    let text = path.to_str().context("Path must be valid UTF-8")?;
    #[cfg(windows)]
    let text = windows_path(text);
    #[cfg(not(windows))]
    let text = Cow::Borrowed(text);
    Ok(text)
}

pub fn launcher_line(path: &Path) -> Result<String> {
    let text = portable(path)?;
    ensure!(
        descriptor_value(&text),
        "Output path cannot be represented in a launcher descriptor"
    );
    Ok(format!("path=\"{text}\""))
}

pub fn descriptor_value(text: &str) -> bool {
    !text.is_empty() && !text.contains(['"', '\\']) && !text.chars().any(char::is_control)
}

#[cfg(any(windows, test))]
fn windows_path(path: &str) -> Cow<'_, str> {
    // canonicalize возвращает verbatim-пути; launcher ожидает обычный путь диска или UNC.
    if let Some(path) = path.strip_prefix(r"\\?\UNC\") {
        return Cow::Owned(format!("//{}", path.replace('\\', "/")));
    }
    let path = path.trim_prefix(r"\\?\");
    if path.contains('\\') {
        Cow::Owned(path.replace('\\', "/"))
    } else {
        Cow::Borrowed(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_paths_preserve_case_and_remove_verbatim_prefixes() {
        for (native, expected) in [
            (r"interface\Example.gui", "interface/Example.gui"),
            (r"C:\Users\Player\Scaled UI", "C:/Users/Player/Scaled UI"),
            (r"\\?\C:\Mods\Scaled UI", "C:/Mods/Scaled UI"),
            (r"\\server\Mods\Scaled UI", "//server/Mods/Scaled UI"),
            (r"\\?\UNC\server\Mods\Scaled UI", "//server/Mods/Scaled UI"),
        ] {
            assert_eq!(windows_path(native), expected);
        }
        std::assert_matches!(windows_path("gfx/Icon.png"), Cow::Borrowed(_));
    }
}
