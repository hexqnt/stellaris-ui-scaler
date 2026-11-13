use std::{collections::BTreeMap, path::Path};

use stellaris_ui_scaler::{
    assets::Installation,
    compiler::{self, Control},
    config::{Profile, Scale},
    fonts, outliner,
    pdx::{Document, Edits, Node},
    scaling,
};

#[test]
#[ignore = "Requires a local game installation; set STELLARIS_GAME"]
fn installed_corpus_roundtrip_and_transform() {
    let path = std::env::var("STELLARIS_GAME").expect("Set STELLARIS_GAME");
    let plan = compiler::plan(Path::new(&path), &Control::default(), &mut |_| {}).unwrap();
    let statuses = plan
        .sprites
        .iter()
        .map(|s| (s.name.clone(), s.convert))
        .collect();
    for factor in [1.3846, 1.86921, 2.0] {
        let scale = Scale::try_from(factor).unwrap();
        for (path, source) in &plan.sources {
            let doc = Document::parse(source).unwrap();
            assert_eq!(Edits::default().apply(doc.source).unwrap(), *source);
            let text = if Path::new(path)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("gui"))
            {
                let text = scaling::geometry(source, scale, false, path, &mut Vec::new()).unwrap();
                let text = scaling::widgets(&text, scale, &statuses).unwrap();
                let text = outliner::shared_gui(
                    &text,
                    path,
                    scale,
                    &stellaris_ui_scaler::config::OutlinerOptions {
                        compact: true,
                        ..Default::default()
                    },
                    &mut Vec::new(),
                )
                .unwrap();
                if path == "interface/outliner.gui" {
                    outliner::transform(&text, &Profile::default().outliner, scale).unwrap()
                } else {
                    text
                }
            } else if path.contains("font") {
                fonts::transform(source, scale).unwrap()
            } else {
                scaling::geometry(source, scale, true, path, &mut Vec::new()).unwrap()
            };
            Document::parse(&text).unwrap_or_else(|e| panic!("{path}: {e}"));
        }
    }
    assert!(plan.sources.len() > 300);
}

fn numbers(nodes: &[Node<'_>], prefix: &str, output: &mut BTreeMap<String, String>) {
    let mut counts = BTreeMap::new();
    for node in nodes {
        let identity = format!(
            "{}:{}",
            node.key.unwrap_or("item"),
            node.named().unwrap_or("")
        );
        let count = counts.entry(identity.clone()).or_insert(0);
        let path = format!("{prefix}/{identity}#{count}");
        *count += 1;
        if let Some(value) = node.atom()
            && value.parse::<f64>().is_ok()
        {
            output.insert(path.clone(), value.parse::<f64>().unwrap().to_string());
        }
        numbers(node.children(), &path, output);
    }
}
#[test]
#[ignore = "Requires fresh Python and Rust reference builds"]
fn reference_geometry_comparison() {
    let game = Installation::open(Path::new(&std::env::var("STELLARIS_GAME").unwrap())).unwrap();
    let python = std::env::var("UI_SCALER_PYTHON_MOD").unwrap();
    let rust = std::env::var("UI_SCALER_RUST_MOD").unwrap();
    let mut differences = Vec::new();
    let mut equal = 0;
    for relative in game.interface_files() {
        let original = game.text(relative).unwrap();
        let left = std::fs::read_to_string(Path::new(&python).join(relative))
            .unwrap_or_else(|_| original.clone());
        let right = std::fs::read_to_string(Path::new(&rust).join(relative)).unwrap_or(original);
        let mut a = BTreeMap::new();
        numbers(&Document::parse(&left).unwrap().nodes, "", &mut a);
        let mut b = BTreeMap::new();
        numbers(&Document::parse(&right).unwrap().nodes, "", &mut b);
        for (key, value) in &a {
            if b.get(key) == Some(value) {
                equal += 1;
            } else {
                differences.push(serde_json::json!({"file":relative,"property":key,"python":value,"rust":b.get(key)}));
            }
        }
    }
    let report = serde_json::json!({"equal_numeric_properties":equal,"differences":differences});
    std::fs::write(
        std::env::temp_dir().join("ui-scaler-geometry-comparison.json"),
        serde_json::to_string_pretty(&report).unwrap(),
    )
    .unwrap();
    println!(
        "Compared {equal} equal numeric properties; {} differences",
        differences.len()
    );
    assert!(equal > 10_000);
}

#[test]
#[ignore = "Requires fresh Python and Rust reference builds"]
fn reference_texture_comparison() {
    let python = std::env::var("UI_SCALER_PYTHON_MOD").unwrap();
    let rust = std::env::var("UI_SCALER_RUST_MOD").unwrap();
    let files = stellaris_ui_scaler::assets::files(&Path::new(&rust).join("gfx")).unwrap();
    let stride = (files.len() / 200).max(1);
    let mut samples = Vec::new();
    for path in files.iter().step_by(stride).take(200) {
        let relative = path.strip_prefix(&rust).unwrap().to_string_lossy();
        let reference = Path::new(&python).join(relative.replace("_scaled.png", "_x14.png"));
        if !reference.is_file() {
            continue;
        }
        let a = image::open(reference).unwrap().to_rgba8();
        let b = image::open(path).unwrap().to_rgba8();
        assert_eq!(a.dimensions(), b.dimensions(), "{relative}");
        let mut total = 0.0;
        let mut weighted = 0.0;
        let mut weights = 0.0;
        for (left, right) in a.pixels().zip(b.pixels()) {
            let alpha = f64::from(left[3].max(right[3])) / 255.0;
            for (x, y) in left.0.into_iter().zip(right.0) {
                total += f64::from(x.abs_diff(y));
            }
            for (x, y) in left.0[..3].iter().zip(&right.0[..3]) {
                weighted = f64::from(x.abs_diff(*y)).mul_add(alpha, weighted);
            }
            weights = alpha.mul_add(3.0, weights);
        }
        let count = f64::from(a.width()) * f64::from(a.height()) * 4.0;
        samples.push(serde_json::json!({"path":relative,"rgba_mean_absolute_error":total/count,"visible_rgb_mean_absolute_error":if weights>0.0 {weighted/weights} else {0.0}}));
    }
    let average = samples
        .iter()
        .map(|s| s["visible_rgb_mean_absolute_error"].as_f64().unwrap())
        .sum::<f64>()
        / f64::from(u32::try_from(samples.len()).unwrap());
    std::fs::write(
        std::env::temp_dir().join("ui-scaler-texture-comparison.json"),
        serde_json::to_string_pretty(&samples).unwrap(),
    )
    .unwrap();
    println!(
        "Compared {} textures; mean visible RGB absolute difference: {average:.3}/255",
        samples.len()
    );
    assert!(samples.len() >= 190);
}
