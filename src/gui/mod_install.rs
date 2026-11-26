use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};

use crate::{
    atomic_file::{AtomicFile, Durability},
    paths,
};

pub(super) fn install(output: &Path, mods_directory: &Path) -> Result<PathBuf> {
    ensure!(
        mods_directory.is_absolute(),
        "Mods directory must be absolute"
    );
    ensure!(
        output.join("descriptor.mod").is_file(),
        "Built mod is missing; rebuild it before installing"
    );
    let source = output.with_added_extension("mod");
    let name = source.file_name().context("Missing mod name")?;
    let descriptor =
        fs::read_to_string(&source).context("Cannot read built launcher descriptor")?;
    let path_line = paths::launcher_line(output)?;
    ensure!(
        descriptor.lines().any(|line| line == path_line),
        "Launcher descriptor does not point to the built mod"
    );
    fs::create_dir_all(mods_directory).context("Cannot create mods directory")?;
    let destination = mods_directory.join(name);
    ensure!(
        !destination.is_symlink(),
        "Installed launcher descriptor must not be a symbolic link"
    );
    if destination.exists() {
        let previous = fs::read_to_string(&destination)?;
        ensure!(
            previous.lines().any(|line| line == path_line),
            "A different mod already uses {}",
            destination.display()
        );
    }
    AtomicFile::prepare(&destination, descriptor.as_bytes(), Durability::Synced)
        .context("Cannot prepare launcher descriptor")?
        .publish()
        .context("Cannot install launcher descriptor")?;
    Ok(destination)
}

pub(super) fn suggested_directory(game_root: &Path) -> Result<PathBuf> {
    crate::user_data::directory(game_root).map(|path| path.join("mod"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installs_updates_and_preserves_other_mods() {
        let directory = crate::test_support::Temp::new();
        let root = directory.0.canonicalize().unwrap();
        let output = root.join("scaled.ui");
        fs::create_dir(&output).unwrap();
        fs::write(output.join("descriptor.mod"), "name=\"Scaled UI\"\n").unwrap();
        let source = root.join("scaled.ui.mod");
        let descriptor = format!(
            "name=\"Scaled UI\"\n{}\n",
            paths::launcher_line(&output).unwrap()
        );
        fs::write(&source, &descriptor).unwrap();
        let mods = root.join("user/mod");
        let installed = install(&output, &mods).unwrap();
        assert_eq!(fs::read_to_string(&installed).unwrap(), descriptor);
        assert!(output.join("descriptor.mod").is_file());
        let updated = descriptor.replace("Scaled UI", "Updated UI");
        fs::write(&source, &updated).unwrap();
        install(&output, &mods).unwrap();
        assert_eq!(fs::read_to_string(&installed).unwrap(), updated);
        fs::write(&installed, "name=\"Other mod\"\npath=\"/other\"\n").unwrap();
        assert!(install(&output, &mods).is_err());
        assert_eq!(
            fs::read_to_string(&installed).unwrap(),
            "name=\"Other mod\"\npath=\"/other\"\n"
        );
        fs::remove_dir_all(&output).unwrap();
        assert!(install(&output, &mods).is_err());
    }
}
