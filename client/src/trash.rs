use std::path::{Path, PathBuf};

use fs4me_interface::{Driver, DriverError};

/// Возвращает путь до корзины для заданного пути.
/// Если корзина не существует, она будет создана.
///
/// @param driver - Драйвер файловой системы.
/// @param path - Путь к файлу или директории.
/// @return Result<PathBuf, DriverError> - Результат: путь до корзины или ошибка.
pub(crate) fn trash_dir_for<P: AsRef<Path>, D: Driver>(
    driver: &D,
    path: P,
) -> Result<PathBuf, DriverError> {
    let path = path.as_ref();
    let parent = path
        .parent()
        .ok_or_else(|| DriverError::ParentDirError(path.to_path_buf()))?;
    let trash = parent.join(".trash");
    if !driver.exists(&trash) {
        driver.mkdir(&trash, false)?;
    }

    Ok(trash)
}

/// Получить уникальный путь для перемещения файла или директории в корзину,
/// избегая конфликтов имен с существующими файлами.
///
/// @param driver - Драйвер файловой системы.
/// @param path - Путь к файлу или директории.
/// @return Result<PathBuf, DriverError> - Результат: путь до уникального файла в корзине или ошибка.
pub(crate) fn trash_unique_path<P: AsRef<Path>, D: Driver>(
    driver: &D,
    path: P,
) -> Result<PathBuf, DriverError> {
    let path = path.as_ref();
    let trash = trash_dir_for(driver, path)?;

    let file_name = path
        .file_prefix()
        .and_then(|name| name.to_str())
        .ok_or_else(|| DriverError::FileNameError(path.to_path_buf()))?;
    let file_ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|ext| format!(".{ext}"))
        .unwrap_or_default();

    let mut new_path = trash.join(format!("{file_name}{file_ext}"));

    if driver.exists(&new_path) {
        let mut count = 1;
        loop {
            new_path = trash.join(format!("{file_name}_{count}{file_ext}"));
            if !driver.exists(&new_path) {
                break;
            }
            count += 1;
        }
    }

    Ok(new_path)
}
