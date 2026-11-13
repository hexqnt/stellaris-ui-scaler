use std::{fmt, path::PathBuf};

/// Расхождение сохранённого списка с экспортированной конфигурацией запуска.
#[derive(Debug)]
pub struct ModListMismatch {
    pub saved: Vec<PathBuf>,
    pub current: Vec<PathBuf>,
}

impl ModListMismatch {
    /// Сравнивает списки до копирования путей; одинаковые списки не требуют аллокаций.
    #[must_use]
    pub fn between(saved: &[PathBuf], current: &[PathBuf]) -> Option<Self> {
        (saved != current).then(|| Self {
            saved: saved.to_vec(),
            current: current.to_vec(),
        })
    }

    /// Представление обоих порядков для ошибок CLI и уведомлений GUI.
    #[must_use]
    pub fn orders(&self) -> impl fmt::Display + '_ {
        Orders {
            saved: &self.saved,
            current: &self.current,
        }
    }
}

impl std::error::Error for ModListMismatch {}
impl fmt::Display for ModListMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "Enabled UI mods or their load order differ from the profile."
        )?;
        write!(f, "{}", self.orders())?;
        write!(
            f,
            "Scan assets again in the GUI to load the current enabled mods, then save the profile. CLI: sync-source-mods --game <GAME> --profile <PROFILE>."
        )
    }
}

struct Orders<'a> {
    saved: &'a [PathBuf],
    current: &'a [PathBuf],
}

impl fmt::Display for Orders<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (label, paths) in [("Saved order", self.saved), ("Current order", self.current)] {
            writeln!(f, "{label}:")?;
            if paths.is_empty() {
                writeln!(f, "  (none)")?;
            }
            for path in paths {
                writeln!(f, "  {}", path.display())?;
            }
        }
        Ok(())
    }
}
