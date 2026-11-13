use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use anyhow::{Context, Result, ensure};
use serde::Serialize;

use crate::{
    assets::{self, Diagnostic, Installation},
    paths,
    pdx::{Document, Node},
};

#[derive(Debug, Default, Serialize)]
pub struct ValidationReport {
    pub checked_files: usize,
    pub errors: Vec<Diagnostic>,
    pub inherited_missing: Vec<Diagnostic>,
}

fn texture_references<'source>(doc: &Document<'source>) -> BTreeSet<Cow<'source, str>> {
    let mut refs = BTreeSet::new();
    doc.walk(|n| {
        if n.key
            .is_some_and(|key| assets::is_texture_key(key) || key.eq_ignore_ascii_case("file"))
            && let Some(value) = n.atom()
        {
            refs.insert(assets::normalize_reference(value));
        }
    });
    refs
}

fn check_references(
    game: &Installation,
    directory: &Path,
    path: &str,
    doc: &Document<'_>,
    original: &Document<'_>,
    report: &mut ValidationReport,
) {
    let baseline = texture_references(original);
    for reference in texture_references(doc) {
        let exists_in_game = game.contains(&reference);
        let valid = assets::safe_relative(&reference).is_ok()
            && (directory.join(reference.as_ref()).is_file() || exists_in_game);
        if valid {
            continue;
        }
        let inherited = baseline.contains(&reference) && !exists_in_game;
        let diagnostic = Diagnostic {
            path: path.into(),
            span: None,
            rule: "missing-reference".into(),
            message: reference.into_owned(),
        };
        if inherited {
            report.inherited_missing.push(diagnostic);
        } else {
            report.errors.push(diagnostic);
        }
    }
}

fn check_frames(directory: &Path, path: &str, doc: &Document<'_>, errors: &mut Vec<Diagnostic>) {
    doc.walk(|node| {
        let Some(texture) = node.field("texturefile").and_then(Node::atom) else {
            return;
        };
        let texture = assets::normalize_reference(texture);
        if !texture.ends_with("_scaled.png") || assets::safe_relative(&texture).is_err() {
            return;
        }
        let target = directory.join(texture.as_ref());
        if !target.is_file() {
            return;
        }
        let frames = node
            .field("noOfFrames")
            .and_then(Node::parse_atom::<u32>)
            .unwrap_or(1);
        match image::image_dimensions(target) {
            Ok((width, _)) if frames > 0 && width.is_multiple_of(frames) => (),
            result => errors.push(Diagnostic {
                path: path.into(),
                span: Some(node.span.clone()),
                rule: "frame-dimensions".into(),
                message: format!("Invalid {frames}-frame texture {texture}: {result:?}"),
            }),
        }
    });
}

fn duplicates(
    nodes: &[Node<'_>],
    ancestry: &str,
    out: &mut BTreeMap<String, std::ops::Range<usize>>,
) {
    for (index, node) in nodes.iter().enumerate() {
        let identity = node.named().unwrap_or_else(|| node.key.unwrap_or("list"));
        let path = format!("{ancestry}/{identity}:{index}");
        let mut seen = BTreeSet::new();
        for child in node.children().iter().filter(|n| n.atom().is_some()) {
            let key = child.key.unwrap_or("").to_ascii_lowercase();
            if !key.is_empty()
                && let Some(key) = seen.replace(key)
            {
                out.insert(format!("{path}/{key}"), child.span.clone());
            }
        }
        duplicates(node.children(), &path, out);
    }
}

/// # Errors
/// Возвращает ошибку доступа к исходным или сгенерированным файлам.
pub fn validate(game: &Installation, directory: &Path) -> Result<ValidationReport> {
    ensure!(
        directory.is_dir(),
        "Mod directory does not exist or is not a directory: {}",
        directory.display()
    );
    let mut report = ValidationReport::default();
    for path in assets::files(directory)? {
        let relative_path = path.strip_prefix(directory)?;
        if ["common", "events", "map"]
            .iter()
            .any(|prefix| relative_path.starts_with(prefix))
        {
            report.errors.push(Diagnostic {
                path: paths::portable(relative_path)?.into_owned(),
                span: None,
                rule: "gameplay-data".into(),
                message: "Generated mod must not change gameplay data".into(),
            });
            continue;
        }
        if !["interface", "fonts"]
            .iter()
            .any(|prefix| relative_path.starts_with(prefix))
            || !path.extension().is_some_and(|extension| {
                ["gui", "gfx", "asset"]
                    .iter()
                    .any(|candidate| extension.eq_ignore_ascii_case(candidate))
            })
        {
            continue;
        }
        let relative = paths::portable(relative_path)?.into_owned();
        let source =
            fs::read_to_string(&path).with_context(|| format!("Cannot read {}", path.display()))?;
        let doc = match Document::parse(&source) {
            Ok(doc) => doc,
            Err(error) => {
                report.errors.push(Diagnostic {
                    path: relative,
                    span: None,
                    rule: "syntax".into(),
                    message: error.to_string(),
                });
                continue;
            }
        };
        report.checked_files += 1;
        let original = if game.contains(&relative) {
            game.text(&relative)?
        } else {
            String::new()
        };
        let original_doc = Document::parse(&original)?;
        check_references(game, directory, &relative, &doc, &original_doc, &mut report);
        let mut baseline_duplicates = BTreeMap::new();
        duplicates(&original_doc.nodes, "", &mut baseline_duplicates);
        let mut generated_duplicates = BTreeMap::new();
        duplicates(&doc.nodes, "", &mut generated_duplicates);
        for (key, span) in generated_duplicates {
            if !baseline_duplicates.contains_key(&key) {
                report.errors.push(Diagnostic {
                    path: relative.clone(),
                    span: Some(span),
                    rule: "duplicate-property".into(),
                    message: key,
                });
            }
        }
        check_frames(directory, &relative, &doc, &mut report.errors);
    }
    Ok(report)
}
