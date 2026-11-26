use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    fs,
    ops::Range,
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
    doc.walk(|node| {
        if let Some((reference, _)) = assets::texture_reference(node) {
            refs.insert(reference);
        } else if node.key.is_some_and(|key| key.eq_ignore_ascii_case("file"))
            && let Some(value) = node.atom()
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
    let mut baseline = None;
    for reference in texture_references(doc) {
        let exists_in_game = game.contains(&reference);
        let valid = assets::safe_relative(&reference).is_ok()
            && (directory.join(reference.as_ref()).is_file() || exists_in_game);
        if valid {
            continue;
        }
        let inherited = !exists_in_game
            && baseline
                .get_or_insert_with(|| texture_references(original))
                .contains(&reference);
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

fn duplicates(doc: &Document<'_>) -> BTreeMap<String, Range<usize>> {
    fn visit<'doc, 'source>(
        nodes: &'doc [Node<'source>],
        ancestry: &mut String,
        properties: &mut Vec<(&'source str, &'doc Range<usize>)>,
        out: &mut BTreeMap<String, Range<usize>>,
    ) {
        for (index, node) in nodes.iter().enumerate() {
            let children = node.children();
            if children.is_empty() {
                continue;
            }
            let prefix_len = ancestry.len();
            let identity = node.named().or(node.key).unwrap_or("list");
            let _ = write!(ancestry, "/{identity}:{index}");
            properties.clear();
            properties.extend(children.iter().filter_map(|child| {
                let key = child.key.filter(|key| !key.is_empty())?;
                child.atom().map(|_| (key, &child.span))
            }));
            properties.sort_unstable_by(|(a, a_span), (b, b_span)| {
                a.bytes()
                    .map(|c| c.to_ascii_lowercase())
                    .cmp(b.bytes().map(|c| c.to_ascii_lowercase()))
                    .then_with(|| a_span.start.cmp(&b_span.start))
            });
            for matches in properties.chunk_by(|(a, _), (b, _)| a.eq_ignore_ascii_case(b)) {
                if let [_, .., (key, span)] = matches {
                    let mut path = format!("{ancestry}/{key}");
                    path[ancestry.len() + 1..].make_ascii_lowercase();
                    out.insert(path, (*span).clone());
                }
            }
            visit(children, ancestry, properties, out);
            ancestry.truncate(prefix_len);
        }
    }

    let mut out = BTreeMap::new();
    visit(&doc.nodes, &mut String::new(), &mut Vec::new(), &mut out);
    out
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
        let generated_duplicates = duplicates(&doc);
        if !generated_duplicates.is_empty() {
            let baseline = duplicates(&original_doc);
            for (key, span) in generated_duplicates {
                if !baseline.contains_key(&key) {
                    report.errors.push(Diagnostic {
                        path: relative.clone(),
                        span: Some(span),
                        rule: "duplicate-property".into(),
                        message: key,
                    });
                }
            }
        }
        check_frames(directory, &relative, &doc, &mut report.errors);
    }
    Ok(report)
}
