use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Подготовленный файл в каталоге назначения; неопубликованный файл удаляется при выходе.
pub struct AtomicFile<'a> {
    temporary: PathBuf,
    destination: &'a Path,
}

#[derive(Clone, Copy)]
pub enum Durability {
    Buffered,
    Synced,
}

impl<'a> AtomicFile<'a> {
    pub fn prepare(
        destination: &'a Path,
        bytes: &[u8],
        durability: Durability,
    ) -> io::Result<Self> {
        Self::prepare_with(destination, |file| {
            file.write_all(bytes)?;
            if matches!(durability, Durability::Synced) {
                file.sync_all()?;
            }
            Ok(())
        })
    }

    fn prepare_with(
        destination: &'a Path,
        write: impl FnOnce(&mut File) -> io::Result<()>,
    ) -> io::Result<Self> {
        let (pending, mut file) = loop {
            let temporary = destination.with_file_name(format!(
                ".ui-scaler-{}-{}.tmp",
                std::process::id(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
            {
                Ok(file) => {
                    break (
                        Self {
                            temporary,
                            destination,
                        },
                        file,
                    );
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
        };
        let result = write(&mut file);
        drop(file);
        result?;
        Ok(pending)
    }

    pub fn publish(self) -> io::Result<()> {
        fs::rename(&self.temporary, self.destination)
    }
}

impl Drop for AtomicFile<'_> {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.temporary);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::Temp;

    #[test]
    fn preparation_preserves_destination_and_cleans_up_on_every_exit() {
        let directory = Temp::new();
        let target = directory.0.join("descriptor.mod");
        fs::write(&target, b"original").unwrap();

        let pending = AtomicFile::prepare(&target, b"replacement", Durability::Buffered).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"original");
        drop(pending);
        assert_eq!(
            crate::assets::files(&directory.0).unwrap().as_slice(),
            std::slice::from_ref(&target)
        );

        let result = AtomicFile::prepare_with(&target, |file| {
            file.write_all(b"partial")?;
            Err(io::Error::other("Write failed"))
        });
        assert!(result.is_err());
        drop(result);
        assert_eq!(fs::read(&target).unwrap(), b"original");
        assert_eq!(
            crate::assets::files(&directory.0).unwrap().as_slice(),
            std::slice::from_ref(&target)
        );

        let blocked = directory.0.join("blocked");
        fs::create_dir(&blocked).unwrap();
        let pending = AtomicFile::prepare(&blocked, b"replacement", Durability::Buffered).unwrap();
        assert!(pending.publish().is_err());
        assert!(blocked.is_dir());
        assert_eq!(
            crate::assets::files(&directory.0).unwrap().as_slice(),
            std::slice::from_ref(&target)
        );

        AtomicFile::prepare(&target, b"replacement", Durability::Buffered)
            .unwrap()
            .publish()
            .unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"replacement");
        assert_eq!(crate::assets::files(&directory.0).unwrap(), [target]);
    }
}
