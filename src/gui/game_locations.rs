use std::{
    env, fs,
    path::{Path, PathBuf},
};

use crate::{paths, pdx};

#[derive(Debug)]
enum Token<'a> {
    Text(&'a str),
    Open,
    Close,
}

/// Проверяет только структуру каталогов, не индексируя ресурсы игры.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct GameLocation(PathBuf);

impl GameLocation {
    fn from_path(path: &Path) -> Option<Self> {
        let root = paths::game_resources(path);
        (root.join("interface").is_dir()
            && root.join("gfx").is_dir()
            && root.join("launcher-settings.json").is_file())
        .then(|| root.canonicalize().ok().map(Self))?
    }

    pub(super) fn path(&self) -> &Path {
        &self.0
    }
}

pub(super) fn discover() -> Vec<GameLocation> {
    let mut steam = Vec::new();
    let mut games = Vec::new();
    if let Some(home) = env::var_os("HOME").map(PathBuf::from) {
        steam.extend([
            home.join(".steam/steam"),
            home.join(".steam/root"),
            home.join(".local/share/Steam"),
            home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"),
            home.join("snap/steam/common/.local/share/Steam"),
            home.join("Library/Application Support/Steam"),
        ]);
        games.push(home.join("ssd/Stellaris"));
        for directory in ["Games", "games", "GOG Games", "Applications"] {
            for name in ["Stellaris", "stellaris"] {
                games.push(home.join(directory).join(name));
            }
        }
    }
    if let Some(data) = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
    {
        steam.push(data.join("Steam"));
    }
    for variable in ["ProgramFiles(x86)", "ProgramFiles", "LOCALAPPDATA"] {
        if let Some(directory) = env::var_os(variable).map(PathBuf::from) {
            steam.push(directory.join("Steam"));
            games.push(directory.join("GOG Galaxy/Games/Stellaris"));
        }
    }
    #[cfg(target_os = "windows")]
    games.push(PathBuf::from(r"C:\GOG Games\Stellaris"));
    discover_in(&steam, games)
}

fn discover_in(steam: &[PathBuf], mut candidates: Vec<PathBuf>) -> Vec<GameLocation> {
    for root in steam {
        candidates.push(root.join("steamapps/common/Stellaris"));
        if let Ok(source) = fs::read_to_string(root.join("steamapps/libraryfolders.vdf")) {
            candidates.extend(
                library_paths(&source)
                    .into_iter()
                    .map(|path| path.join("steamapps/common/Stellaris")),
            );
        }
    }
    let mut found = Vec::new();
    for path in candidates {
        if let Some(location) = GameLocation::from_path(&path)
            && !found.contains(&location)
        {
            found.push(location);
        }
    }
    // Порядок кандидатов задаёт приоритет: ~/ssd/Stellaris проверяется первым.
    found
}

/// VDF использует строки в кавычках, пары без знака равенства и комментарии с //.
fn tokens(mut source: &str) -> impl Iterator<Item = Result<Token<'_>, ()>> {
    std::iter::from_fn(move || {
        loop {
            source = source.trim_start();
            if !source.starts_with("//") {
                break;
            }
            source = source.split_once('\n').map_or("", |(_, tail)| tail);
        }
        let token = match source.as_bytes().first()? {
            b'{' => Some((Token::Open, 1)),
            b'}' => Some((Token::Close, 1)),
            b'"' => pdx::quoted_end(&source.as_bytes()[1..])
                .map(|end| (Token::Text(&source[1..=end]), end + 2)),
            _ => None,
        };
        let Some((token, length)) = token else {
            source = "";
            return Some(Err(()));
        };
        source = &source[length..];
        Some(Ok(token))
    })
}

fn library_paths(source: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let mut depth = 0_u32;
    let mut libraries = None;
    let mut entry_depth = None;
    let mut tokens = tokens(source);
    while let Some(token) = tokens.next() {
        match token {
            Ok(Token::Text(key)) => match tokens.next() {
                Some(Ok(Token::Open)) => {
                    if depth == 0 && key.eq_ignore_ascii_case("libraryfolders") {
                        libraries = Some(depth + 1);
                    }
                    if libraries == Some(depth) && key.parse::<u32>().is_ok() {
                        entry_depth = Some(depth + 1);
                    }
                    depth += 1;
                }
                Some(Ok(Token::Text(value))) => {
                    let legacy = libraries == Some(depth) && key.parse::<u32>().is_ok();
                    let modern = entry_depth == Some(depth) && key.eq_ignore_ascii_case("path");
                    if legacy || modern {
                        let path = PathBuf::from(pdx::unescape(value).as_ref());
                        if path.is_absolute() {
                            paths.push(path);
                        }
                    }
                }
                _ => return Vec::new(),
            },
            Ok(Token::Close) if depth > 0 => {
                if libraries == Some(depth) {
                    libraries = None;
                }
                if entry_depth == Some(depth) {
                    entry_depth = None;
                }
                depth -= 1;
            }
            _ => return Vec::new(),
        }
    }
    if depth == 0 { paths } else { Vec::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_modern_and_legacy_libraries() {
        let prefix = if cfg!(windows) { "C:" } else { "" };
        let modern = format!("{prefix}/mnt/Steam Library");
        let legacy = format!("{prefix}/games");
        let paths = library_paths(&format!(
            r#""libraryfolders" {{ "0" {{ "apps" {{ "281990" "123" }} "path" "{modern}" }} "1" "{legacy}" "TimeNextStatsReport" "123" }}"#,
        ));
        assert_eq!(paths, [PathBuf::from(&modern), PathBuf::from(&legacy)]);
        assert_eq!(
            library_paths(&format!(
                r#""libraryfolders" {{ "0" {{ "path" "{legacy}" }}"#
            )),
            Vec::<PathBuf>::new()
        );
        assert_eq!(
            library_paths(r#""libraryfolders" { "0" "relative" }"#),
            Vec::<PathBuf>::new()
        );
        assert_eq!(
            library_paths(&format!(r#""other" {{ "path" "{legacy}" }}"#)),
            Vec::<PathBuf>::new()
        );
    }

    #[test]
    fn decodes_windows_paths() {
        let tokens = tokens(
            r#""path" "D:\\SteamLibrary" // comment
        "name" "Some \"name\"""#,
        )
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
        std::assert_matches!(&tokens[1], Token::Text(value) if pdx::unescape(value) == r"D:\SteamLibrary");
        std::assert_matches!(&tokens[3], Token::Text(value) if pdx::unescape(value) == "Some \"name\"");
    }

    #[test]
    fn discovers_libraries_and_gog_without_duplicates_or_incomplete_installs() {
        let directory = crate::test_support::Temp::new();
        let root = &directory.0;
        let steam = root.join("Steam");
        let library = root.join("External Library");
        let game = library.join("steamapps/common/Stellaris");
        let gog = root.join("GOG Games/Stellaris/game");
        for path in [&game, &gog] {
            fs::create_dir_all(path.join("interface")).unwrap();
            fs::create_dir_all(path.join("gfx")).unwrap();
            fs::write(path.join("launcher-settings.json"), "{}").unwrap();
        }
        fs::create_dir_all(steam.join("steamapps/common/Stellaris")).unwrap();
        fs::write(
            steam.join("steamapps/libraryfolders.vdf"),
            format!(
                "\"libraryfolders\" {{ \"0\" {{ \"path\" \"{}\" }} }}",
                library.display()
            ),
        )
        .unwrap();
        let found = discover_in(
            &[steam, root.join("missing")],
            vec![root.join("GOG Games/Stellaris"), game.clone()],
        );
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].path(), gog.canonicalize().unwrap());
        assert!(found.contains(&GameLocation(game.canonicalize().unwrap())));
        assert!(found.contains(&GameLocation(gog.canonicalize().unwrap())));
    }
}
