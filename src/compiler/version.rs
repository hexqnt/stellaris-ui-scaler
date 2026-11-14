use crate::assets::Diagnostic;

pub(super) const RULE: &str = "game-version";
const TARGET_LINE: (u32, u32) = (4, 5);

fn release_line(version: &str) -> Option<(u32, u32)> {
    let mut parts = version.strip_prefix('v').unwrap_or(version).split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    parts.next()?.parse::<u32>().ok()?;
    parts.next().is_none().then_some((major, minor))
}

pub(super) fn warning(version: &str) -> Option<Diagnostic> {
    let message = match release_line(version) {
        Some(TARGET_LINE) => return None,
        Some(_) => format!(
            "Game version {version} is outside the targeted v4.5.x series. Processing will continue; check the generated interface in the game."
        ),
        None => format!(
            "Cannot determine the release series of game version {version:?}. Processing will continue; check the generated interface in the game."
        ),
    };
    Some(Diagnostic {
        path: "launcher-settings.json".into(),
        span: None,
        rule: RULE.into(),
        message,
    })
}
