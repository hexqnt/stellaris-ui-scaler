use std::{
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;

pub fn directory(game_root: &Path) -> Result<PathBuf> {
    #[derive(Deserialize)]
    struct Settings {
        #[serde(rename = "gameDataPath")]
        game_data_path: String,
    }
    let root = crate::paths::game_resources(game_root);
    let settings: Settings = serde_json::from_slice(
        &fs::read(root.join("launcher-settings.json"))
            .context("Cannot read launcher settings; specify the user data directory manually")?,
    )?;
    expand_data_path(&settings.game_data_path, |name| {
        env::var_os(name).map(PathBuf::from)
    })
}

fn expand_data_path(path: &str, variable: impl Fn(&str) -> Option<PathBuf>) -> Result<PathBuf> {
    let expanded = if let Some(suffix) = path.strip_prefix("$LINUX_DATA_HOME/") {
        let base = variable("XDG_DATA_HOME")
            .filter(|path| path.is_absolute())
            .or_else(|| variable("HOME").map(|home| home.join(".local/share")))
            .context("Cannot locate Linux user data directory")?;
        base.join(suffix)
    } else if let Some(suffix) = path.strip_prefix("$HOME/") {
        variable("HOME").context("HOME is not set")?.join(suffix)
    } else {
        ensure!(
            !path.contains('$'),
            "Unsupported gameDataPath variable; specify the user data directory manually"
        );
        PathBuf::from(path)
    };
    if !expanded.is_absolute() {
        bail!(
            "gameDataPath must resolve to an absolute path; specify the user data directory manually"
        );
    }
    Ok(expanded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_launcher_data_paths() {
        let home = env::temp_dir().join("player");
        let data = env::temp_dir().join("data");
        let resolve = |xdg: Option<&Path>, path: &str| {
            expand_data_path(path, |name| match name {
                "HOME" => Some(home.clone()),
                "XDG_DATA_HOME" => xdg.map(Path::to_path_buf),
                _ => None,
            })
        };
        assert_eq!(
            resolve(None, "$LINUX_DATA_HOME/Paradox Interactive/Stellaris").unwrap(),
            home.join(".local/share/Paradox Interactive/Stellaris")
        );
        assert_eq!(
            resolve(Some(&data), "$LINUX_DATA_HOME/Stellaris").unwrap(),
            data.join("Stellaris")
        );
        assert_eq!(
            resolve(Some(Path::new("relative")), "$LINUX_DATA_HOME/Stellaris").unwrap(),
            home.join(".local/share/Stellaris")
        );
        assert_eq!(
            resolve(None, "$HOME/games/Stellaris").unwrap(),
            home.join("games/Stellaris")
        );
        assert_eq!(
            resolve(None, data.join("Stellaris").to_str().unwrap()).unwrap(),
            data.join("Stellaris")
        );
        assert!(resolve(None, "$UNKNOWN/Stellaris").is_err());
        assert!(resolve(None, "relative/Stellaris").is_err());
    }
}
