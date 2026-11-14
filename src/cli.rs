use std::{
    fs,
    io::Write,
    num::NonZeroUsize,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use clap::{Args, Parser, Subcommand};
use serde::Serialize;

use crate::{
    assets::Installation,
    atomic_file::AtomicFile,
    compiler::{self, Control, Progress},
    config::{Profile, Scale},
    source_mods, validation,
};

#[derive(Subcommand)]
enum Command {
    #[command(about = "Save the launcher's enabled UI mod list in a TOML profile")]
    SyncSourceMods {
        #[arg(long)]
        game: PathBuf,
        #[arg(long)]
        profile: PathBuf,
        #[arg(long)]
        user_data_dir: Option<PathBuf>,
    },
    Scan {
        #[command(flatten)]
        sources: SourceArgs,
        #[arg(long)]
        profile: Option<PathBuf>,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    Build {
        #[command(flatten)]
        sources: SourceArgs,
        #[command(flatten)]
        settings: ProfileArgs,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        compact: bool,
        #[arg(long)]
        picture: Option<PathBuf>,
        #[arg(long)]
        report: Option<PathBuf>,
    },
    Validate {
        #[command(flatten)]
        sources: SourceArgs,
        #[arg(long = "mod")]
        directory: PathBuf,
    },
    Benchmark {
        #[command(flatten)]
        sources: SourceArgs,
        #[command(flatten)]
        settings: ProfileArgs,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value = "12")]
        samples: NonZeroUsize,
    },
    Gui {
        #[arg(long)]
        game: Option<PathBuf>,
    },
}
#[derive(Args)]
struct SourceArgs {
    #[arg(long)]
    game: PathBuf,
    #[arg(
        long = "source-mod",
        value_name = "DIRECTORY",
        help = "Unpacked UI mod directory; repeat in load order"
    )]
    source_mods: Vec<PathBuf>,
    #[arg(
        long,
        help = "Detect enabled UI mods before scanning; dlc_load.json may reflect a previous launch"
    )]
    auto_source_mods: bool,
    #[arg(
        long,
        conflicts_with = "auto_source_mods",
        help = "Disable automatic source mod discovery from the profile"
    )]
    no_auto_source_mods: bool,
    #[arg(
        long,
        requires = "auto_source_mods",
        help = "Stellaris user data directory; detected from launcher settings by default"
    )]
    user_data_dir: Option<PathBuf>,
}

impl SourceArgs {
    fn apply(self, profile: &mut Profile) -> Result<PathBuf> {
        if self.no_auto_source_mods {
            profile.auto_source_mods = None;
        } else if self.auto_source_mods {
            let directory = self.user_data_dir.as_deref().or_else(|| {
                profile
                    .auto_source_mods
                    .as_ref()
                    .and_then(|settings| settings.user_data_dir.as_deref())
            });
            let discovered = source_mods::discover(&self.game, directory)?;
            if let Some(changes) = profile.auto_source_mods.as_ref().and_then(|settings| {
                source_mods::ModListMismatch::between(
                    &settings.source_mods,
                    &discovered.source_mods,
                )
            }) {
                eprintln!("{changes}");
                eprintln!(
                    "Using the refreshed list for this invocation; the input profile is unchanged."
                );
            }
            eprintln!(
                "Detected {} UI mods; skipped {} non-UI or generated mods. dlc_load.json may reflect a previous launch.",
                discovered.source_mods.len(),
                discovered.skipped_mods
            );
            let settings = profile.auto_source_mods.get_or_insert_default();
            if let Some(directory) = self.user_data_dir {
                settings.user_data_dir = Some(directory);
            }
            settings.source_mods = discovered.source_mods;
        }
        profile.source_mods.extend(self.source_mods);
        profile.check()?;
        Ok(self.game)
    }
}

#[derive(Args)]
struct ProfileArgs {
    #[arg(long)]
    profile: Option<PathBuf>,
    #[arg(long)]
    scale: Option<Scale>,
}

impl ProfileArgs {
    fn load(self) -> Result<Profile> {
        let mut profile = self
            .profile
            .as_deref()
            .map_or_else(|| Ok(Profile::default()), Profile::load)?;
        if let Some(scale) = self.scale {
            profile.scale = scale;
        }
        profile.check()?;
        Ok(profile)
    }
}

#[derive(Parser)]
#[command(
    version,
    about = "Compile sharp Stellaris UI mods from original game assets"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}
fn json<T: Serialize>(value: &T, output: Option<&Path>) -> Result<()> {
    let text = serde_json::to_string_pretty(value)?;
    if let Some(output) = output {
        AtomicFile::prepare(output, |file| file.write_all(text.as_bytes()))
            .and_then(AtomicFile::publish)
            .with_context(|| format!("Cannot write JSON report: {}", output.display()))?;
    } else {
        println!("{text}");
    }
    Ok(())
}
#[allow(
    clippy::needless_pass_by_value,
    reason = "Сигнатура общего callback передаёт событие во владение"
)]
fn progress(event: Progress) {
    if event.completed.is_multiple_of(100) || event.completed == event.total {
        eprintln!("{}: {}/{}", event.stage, event.completed, event.total);
    }
}
fn scan(game: &Path, profile: &Profile, control: &Control) -> Result<compiler::BuildPlan> {
    let plan = compiler::plan_with_profile(game, profile, control, &mut progress)?;
    if let Some(warning) = plan.version_warning() {
        eprintln!("Warning: {}", warning.message);
    }
    Ok(plan)
}

/// # Errors
/// Возвращает ошибку выбранной команды или запуска GUI.
pub fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command.unwrap_or(Command::Gui { game: None }) {
        Command::SyncSourceMods {
            game,
            profile: path,
            user_data_dir,
        } => {
            let mut profile = Profile::load(&path)?;
            let settings = profile.auto_source_mods.get_or_insert_default();
            if let Some(directory) = user_data_dir {
                settings.user_data_dir = Some(directory);
            }
            let discovered = source_mods::refresh(&game, settings)?;
            profile.save(&path)?;
            json(&discovered, None)
        }
        Command::Scan {
            sources,
            profile,
            output,
        } => {
            let mut profile = profile
                .as_deref()
                .map_or_else(|| Ok(Profile::default()), Profile::load)?;
            let game = sources.apply(&mut profile)?;
            let plan = scan(&game, &profile, &Control::default())?;
            json(&plan.report(), output.as_deref())
        }
        Command::Build {
            sources,
            settings,
            output,
            compact,
            picture,
            report,
        } => {
            let mut profile = settings.load()?;
            let game = sources.apply(&mut profile)?;
            if compact {
                profile.outliner.compact = true;
            }
            if let Some(picture) = picture {
                profile.picture = Some(picture);
            }
            let control = Control::default();
            let plan = scan(&game, &profile, &control)?;
            let result = compiler::build(&plan, &profile, &output, &control, &mut progress)?;
            json(&result, report.as_deref())
        }
        Command::Validate { sources, directory } => {
            let mut profile = Profile::default();
            let game = sources.apply(&mut profile)?;
            let source_mods = source_mods::resolve(&game, &profile)?;
            let game = Installation::open(&game)?.with_source_mods(&source_mods)?;
            let report = validation::validate(&game, &directory)?;
            json(&report, None)?;
            ensure!(report.errors.is_empty(), "Mod validation failed");
            Ok(())
        }
        Command::Benchmark {
            sources,
            settings,
            output,
            samples,
        } => {
            ensure!(!output.exists(), "Benchmark output already exists");
            let mut profile = settings.load()?;
            let game = sources.apply(&mut profile)?;
            let plan =
                compiler::plan_with_profile(&game, &profile, &Control::default(), &mut progress)?;
            fs::create_dir_all(&output)?;
            let mut measurements = Vec::new();
            for (index, asset) in plan
                .textures
                .iter()
                .step_by((plan.textures.len() / samples).max(1))
                .take(samples.get())
                .enumerate()
            {
                let source = crate::textures::decode(&plan.game.read(&asset.path)?)?;
                source.save(output.join(format!("{index:04}-source.png")))?;
                for algorithm in crate::config::Algorithm::ALL {
                    let start = std::time::Instant::now();
                    let mut candidates = compiler::preview_candidates(
                        &plan,
                        &profile,
                        asset,
                        &[algorithm],
                        &Control::default(),
                    )?;
                    let (_, image) = candidates.pop().context("Missing preview candidate")?;
                    image.save(output.join(format!("{index:04}-{algorithm:?}.png")))?;
                    measurements.push(serde_json::json!({"asset":asset.path,"algorithm":algorithm,"milliseconds":start.elapsed().as_millis(),"dimensions":image.dimensions()}));
                }
            }
            json(&measurements, Some(&output.join("report.json")))
        }
        Command::Gui { game } => launch_gui(game),
    }
}
#[cfg(feature = "gui")]
fn launch_gui(game: Option<PathBuf>) -> Result<()> {
    crate::gui::run(game)
}
#[cfg(not(feature = "gui"))]
fn launch_gui(_: Option<PathBuf>) -> Result<()> {
    anyhow::bail!("GUI support is disabled; rebuild with --features gui")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_sources_are_opt_in_and_custom_data_requires_them() {
        let parse = |extra: &[&str]| {
            Cli::try_parse_from(
                ["ui-scaler", "scan", "--game", "unused"]
                    .into_iter()
                    .chain(extra.iter().copied()),
            )
        };
        let Some(Command::Scan { sources, .. }) = parse(&[]).unwrap().command else {
            panic!("Expected scan arguments");
        };
        let mut profile = Profile {
            source_mods: vec!["profile-mod".into()],
            ..Profile::default()
        };
        let game = sources.apply(&mut profile).unwrap();
        assert_eq!(game, Path::new("unused"));
        assert_eq!(profile.source_mods, [PathBuf::from("profile-mod")]);
        assert!(profile.auto_source_mods.is_none());
        assert!(parse(&["--user-data-dir", "unused"]).is_err());
        for extra in [
            vec!["--auto-source-mods"],
            vec!["--auto-source-mods", "--user-data-dir", "unused"],
        ] {
            let Some(Command::Scan { sources, .. }) = parse(&extra).unwrap().command else {
                panic!("Expected scan arguments");
            };
            assert!(sources.auto_source_mods);
        }
        profile.auto_source_mods = Some(crate::config::AutoSourceMods::default());
        let Some(Command::Scan { sources, .. }) =
            parse(&["--no-auto-source-mods"]).unwrap().command
        else {
            panic!("Expected scan arguments");
        };
        sources.apply(&mut profile).unwrap();
        assert!(profile.auto_source_mods.is_none());
        assert_eq!(profile.source_mods, [PathBuf::from("profile-mod")]);
        assert!(parse(&["--auto-source-mods", "--no-auto-source-mods"]).is_err());
    }

    #[test]
    fn numeric_arguments_are_parsed_before_accessing_the_game() {
        for command in ["build", "benchmark"] {
            for invalid in ["NaN", "inf", "0", "0.24", "8.01"] {
                assert!(
                    Cli::try_parse_from([
                        "ui-scaler",
                        command,
                        "--game",
                        "unused",
                        "--output",
                        "unused",
                        "--scale",
                        invalid,
                    ])
                    .is_err(),
                    "{command}: {invalid}"
                );
            }
        }
        assert!(
            Cli::try_parse_from([
                "ui-scaler",
                "benchmark",
                "--game",
                "unused",
                "--output",
                "unused",
                "--samples",
                "0",
            ])
            .is_err()
        );
        let cli = Cli::try_parse_from([
            "ui-scaler",
            "benchmark",
            "--game",
            "unused",
            "--output",
            "unused",
            "--scale",
            "1.5",
        ])
        .unwrap();
        let Some(Command::Benchmark {
            settings: ProfileArgs {
                scale: Some(scale), ..
            },
            samples,
            ..
        }) = cli.command
        else {
            panic!("Expected benchmark arguments");
        };
        assert_eq!(scale.get(), 1.5);
        assert_eq!(samples.get(), 12);
    }
}
