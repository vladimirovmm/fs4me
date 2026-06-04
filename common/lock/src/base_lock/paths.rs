use fs4me_interface::DriverError;
use rand::{RngExt, distr::Alphanumeric};
use std::path::{Path, PathBuf};

use crate::helpers::parent_dir;

/// Пути, используемые для реализации блокировки
pub struct LockPaths {
    /// Изначальный путь до файла блокировки.
    pub multi: PathBuf,
    /// Путь до файла блокировки, в который переименовывается в момент успешной блокировки path->block_path.
    pub base: PathBuf,
    /// Временный путь для нового содержимого файла блокировки.
    /// После завершения записи содержимое этого файла атомарно перемещается на место основного файла блокировки. tmp_path->path
    pub tmp_path: PathBuf,
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

        let path = parent.join(&lock_file_name);
        let block_path = parent.join(format!("~{lock_file_name}"));

        let mut rng = rand::rng();
        let tmp_path = parent.join(format!(
            "~{lock_file_name}.{}",
            (0..9)
                .map(|_| rng.sample(Alphanumeric) as char)
                .collect::<String>()
        ));
        Ok(Self {
            multi: path,
            base: block_path,
            tmp_path,
        })
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
