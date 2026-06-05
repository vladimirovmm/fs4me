use fs4me_interface::DriverError;
use std::path::{Path, PathBuf};

/// Возвращает путь к файлу для мультипоточной блокировки
///
/// # Arguments
///
/// * `source_path` - путь к файлу, для которого нужно получить путь к файлу блокировки
pub fn multi_lock_path<P: AsRef<Path>>(source_path: P) -> Result<PathBuf, DriverError> {
    let source_path = source_path.as_ref();
    let source_file_name = source_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| DriverError::FileNameError(source_path.to_path_buf()))?;

    let lock_file_name = if source_file_name.starts_with(".") {
        format!("{}.lock", source_file_name)
    } else {
        format!(".{}.lock", source_file_name)
    };

    Ok(source_path.with_file_name(&lock_file_name))
}

/// Возвращает путь к файлу для базовой блокировки
///
/// # Arguments
///
/// * `source_path` - путь к файлу, для которого нужно получить путь к файлу блокировки
pub fn base_lock_path<P: AsRef<Path>>(source_path: P) -> Result<PathBuf, DriverError> {
    let source_path = source_path.as_ref();
    let source_file_name = source_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| DriverError::FileNameError(source_path.to_path_buf()))?;

    Ok(source_path.with_file_name(format!("~{source_file_name}")))
}
