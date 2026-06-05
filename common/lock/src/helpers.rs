use fs4me_interface::{Driver, DriverError};
use std::{fmt::Debug, path::Path, sync::Arc, time::Duration};
use tracing::{debug, instrument, warn};

/// Время ожидания (в секундах), после которого блокировка считается истекшей.
pub(crate) fn time_expired() -> Duration {
    #[cfg(feature = "test_env")]
    {
        // В тестовом режиме возвращаем 3 секунды
        Duration::from_secs(3)
    }
    #[cfg(not(feature = "test_env"))]
    {
        // В обычном режиме возвращаем 5 минут

        use std::time::Duration;
        Duration::from_mins(5)
    }
}

/// Возвращает родительскую директорию для указанного пути.
///
/// @param path Путь к файлу/директории.
/// @returns Путь к родительской директории.
pub(crate) fn parent_dir(path: &Path) -> Result<&Path, DriverError> {
    path.parent()
        .ok_or_else(|| DriverError::ParentDirError(path.to_path_buf()))
}

/// Проверяет, существует ли родительская директория для указанного пути.
///
/// @return Возвращает `Ok` в случае успеха, или `Err` в случае ошибки.
#[instrument(level = "debug", skip(driver))]
pub(crate) fn parent_dir_mast_exists<D, P>(driver: Arc<D>, path: P) -> Result<(), DriverError>
where
    D: Driver,
    P: AsRef<Path> + Debug,
{
    let path = path.as_ref();
    parent_dir(path).and_then(|path| {
        if driver.exists(path) {
            debug!("Родительская директория существует: {path:?}");
            Ok(())
        } else {
            warn!("Родительская директория не существует: {path:?}");
            Err(DriverError::ParentDirError(path.to_path_buf()))
        }
    })
}
