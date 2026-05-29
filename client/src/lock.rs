use std::{
    collections::HashMap,
    fmt::format,
    io,
    path::{Path, PathBuf},
    str::FromStr,
};

use fs4me_interface::{Driver, DriverError};

use crate::Fs;

pub enum LockMode {
    Read,
    Write,
}

/// Получить путь к файлу блокировки для указанного пути.
///
/// @param path Путь к файлу.
/// @returns Путь к файлу блокировки.
fn lock_path<P: AsRef<Path>>(path: P) -> Result<PathBuf, DriverError> {
    let path = path.as_ref();
    let parent = path
        .parent()
        .ok_or_else(|| DriverError::ParentDirError(path.to_path_buf()))?;
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| DriverError::FileNameError(path.to_path_buf()))?;

    let new_file_name = if file_name.starts_with(".") {
        format!("{}.lock", file_name)
    } else {
        format!(".{}.lock", file_name)
    };

    Ok(parent.join(new_file_name))
}

fn read_lock_stat<D: Driver, P: AsRef<Path>>(
    fs: &Fs<D>,
    path: P,
) -> Result<Option<LockStat>, DriverError> {
    let lock_file = lock_path(path)?;
    if !fs.exists(&lock_file) {
        return Ok(None);
    }
    let lock_reader = fs.driver.read(&lock_file, 0)?;
    let lock_content =
        io::read_to_string(lock_reader).map_err(|err| DriverError::ReadSeekError {
            path: lock_file.to_path_buf(),
            reason: err.to_string(),
        })?;
    // let lock_stat = LockStat::from_string(&lock_content)?;
    Ok(Some(lock_stat))
}

/// Проверяет, заблокирован ли файл для чтения или записи.
///
/// @param fs Файловая система.
/// @param path Путь к файлу.
/// @param mode Режим блокировки.
/// @returns `true`, если файл заблокирован, `false` — если нет.
pub(crate) fn is_locked<D: Driver, P: AsRef<Path>>(
    fs: &Fs<D>,
    path: P,
    mode: LockMode,
) -> Result<bool, DriverError> {
    match read_lock_stat(fs, path)? {
        Some(stat) => {
            todo!()
        }
        None => Ok(false),
    }
}

/// Содержимое файла блокировки.
#[derive(Debug, Default)]
pub struct LockStat {
    /// Список читателей.
    /// Ключ — идентификатор читателя, значение — время блокировки файла. Время берётся с сервера.
    read: HashMap<u64, u32>,
    /// Блокировка на запись.
    /// Ключ — идентификатор писателя, значение — время блокировки файла. Время берётся с сервера.
    write: Option<(u64, u32)>,
}
/// Ожидатеся формат строки ввиде
/// uuid=unixtime=mode
///
/// Например: `0000000000001=1620000000=read`
impl FromStr for LockStat {
    type Err = DriverError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        todo!()
    }
}
