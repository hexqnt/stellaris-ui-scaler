use std::{collections::BTreeMap, path::PathBuf, time::Instant};

use anyhow::Result;
use clap::Parser;
use stellaris_ui_scaler::{
    compiler,
    config::{Profile, Scale},
    textures,
};

#[derive(Parser)]
struct Args {
    #[arg(long)]
    game: PathBuf,
    #[arg(long)]
    output: PathBuf,
    #[arg(long)]
    sources: Option<PathBuf>,
    #[arg(long, default_value = "128")]
    samples: std::num::NonZeroUsize,
    #[arg(long, default_value = "2.0")]
    scale: Scale,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let profile = Profile {
        auto_algorithm: true,
        auto_sharpening: true,
        scale: args.scale,
        ..Profile::default()
    };
    let plan = compiler::plan(&args.game, &compiler::Control::default(), &mut |_| {})?;
    let mut counts = BTreeMap::new();
    let mut rows = Vec::new();
    if let Some(directory) = &args.sources {
        std::fs::create_dir_all(directory)?;
    }
    for asset in plan
        .textures
        .iter()
        .step_by((plan.textures.len() / args.samples).max(1))
        .take(args.samples.get())
    {
        let source = textures::decode(&plan.game.read(&asset.path)?)?;
        if let Some(directory) = &args.sources {
            source.save(directory.join(format!("{:04}.png", rows.len())))?;
        }
        let started = Instant::now();
        let features = asset.analyze(&profile, &source)?;
        let processing = asset.processing(&profile, features.as_ref());
        let reason = processing
            .recommendation
            .map_or("profile", |choice| choice.reason);
        *counts.entry(reason).or_insert(0_usize) += 1;
        rows.push(serde_json::json!({
            "path": asset.path, "category": asset.category, "frame_size": asset.frame_size(),
            "frames": asset.frames, "algorithm": processing.options.algorithm,
            "reason": reason, "sharpening": processing.options.sharpening,
            "analysis_microseconds": started.elapsed().as_micros(),
            "features": features,
        }));
    }
    std::fs::write(
        &args.output,
        serde_json::to_vec_pretty(&serde_json::json!({
            "game_version": plan.game.version, "scale": profile.scale,
            "total_textures": plan.textures.len(), "sampled_textures": rows.len(),
            "counts": counts, "assets": rows,
        }))?,
    )?;
    println!("{}", serde_json::to_string_pretty(&counts)?);
    Ok(())
}
