use fs4me_interface::DriverError;
use std::path::{Path, PathBuf};

use crate::helpers::parent_dir;

/// Пути, используемые для реализации блокировки
pub struct LockPaths {
    /// Для мультипоточной блокировки
    pub multi: PathBuf,
    /// Путь до файла блокировки
    pub base: PathBuf,
}

impl TryFrom<&Path> for LockPaths {
    type Error = DriverError;

    fn try_from(source_path: &Path) -> Result<Self, Self::Error> {
        let parent = parent_dir(source_path)?;
        let source_file_name = source_path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| DriverError::FileNameError(source_path.to_path_buf()))?;

        let lock_file_name = if source_file_name.starts_with(".") {
            format!("{}.lock", source_file_name)
        } else {
            format!(".{}.lock", source_file_name)
        };

        let multi = parent.join(&lock_file_name);
        let base = parent.join(format!("~{lock_file_name}"));

        Ok(Self { multi, base })
    }
}

impl TryFrom<&PathBuf> for LockPaths {
    type Error = DriverError;

    fn try_from(source_path: &PathBuf) -> Result<Self, Self::Error> {
        source_path.as_path().try_into()
    }
}

impl TryFrom<PathBuf> for LockPaths {
    type Error = DriverError;

    fn try_from(source_path: PathBuf) -> Result<Self, Self::Error> {
        source_path.as_path().try_into()
    }
}
