use fs4me_interface::{Driver, DriverError, WriteMode};
use fs4me_uuid::FsUuid;
use std::{
    fmt::{Debug, Display},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use tracing::{debug, instrument, warn};

use crate::helpers::{parent_dir_mast_exists, time_expired};

pub mod paths;
pub use crate::base_lock::paths::LockPaths;

/// Блокировка, предоставляющая эксклюзивный доступ к файлу и исключающая параллельное обращение к нему.
#[derive(Debug)]
pub struct BaseLock<D: Driver + 'static> {
    /// Уникальный идентификатор клиента.
    /// Используется для отображения в логах.
    uuid: FsUuid,
    /// Драйвер для работы с файловой системой.
    driver: Arc<D>,
    /// Обработчик потока, который обновляет время блокировки.
    handle: Option<JoinHandle<Result<(), DriverError>>>,
    /// Флаг, указывающий на остановку потока обновления времени блокировки.
    stop: Arc<AtomicBool>,
    /// Путь до файла блокировки
    pub path: PathBuf,
}

impl<D: Driver> Display for BaseLock<D> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{:?}", self.uuid, self.path)
    }
}

impl<D: Driver> BaseLock<D> {
    /// Пытаемся создать блокировку для указанного пути и заблокировать файл блокировки.
    #[instrument(level = "debug", skip(driver))]
    pub fn try_lock<P>(uuid: FsUuid, driver: Arc<D>, source_path: P) -> Result<Self, DriverError>
    where
        P: AsRef<Path> + Debug,
    {
        let LockPaths { base: path, .. } = source_path.as_ref().try_into()?;
        parent_dir_mast_exists(driver.clone(), &path)?;

        debug!(?uuid, ?path, "Попытка создать файл блокировки");

        let result = || -> Result<(), DriverError> {
            let mut writer = driver.write(&path, WriteMode::FailIfExists)?;
            write!(writer, "{}", uuid).map_err(|err| DriverError::WriteError {
                path: path.clone(),
                reason: err.to_string(),
            })?;
            writer.flush().map_err(|err| DriverError::WriteError {
                path: path.clone(),
                reason: err.to_string(),
            })?;
            Ok(())
        }();
        if let Err(err) = result {
            debug!(
                ?err,
                "Ошибка при создании файла блокировки, повторная попытка"
            );
            // Проверяем, не заблокирован ли файл
            if Self::is_locked(driver.clone(), &path) {
                debug!("Файл заблокирован");
                return Err(err);
            }
            debug!("Удаление файла блокировки, если он существует");
            // Удаляем файл блокировки, если он существует
            driver.rm(&path)?;
            return Self::try_lock(uuid, driver, source_path);
        }

        debug!(
            "Чтение содержимого файла блокировки что бы проверить пренадлежность текущему клиенту"
        );
        let mut reader = driver.read(&path, 0)?;
        let mut content = String::new();
        reader
            .read_to_string(&mut content)
            .map_err(|err| DriverError::ReadSeekError {
                path: path.clone(),
                reason: err.to_string(),
            })?;

        if content.trim() != uuid.to_string() {
            debug!("Блокировка файла принадлежит другому клиенту");
            return Err(DriverError::LockFileBLocked(path.clone()));
        }

        debug!("Инициализация потока обновления времени блокировки");
        let stop = Arc::new(AtomicBool::new(false));
        let interval = Duration::from_secs(15);

        let driver_thread = driver.clone();
        let stop_thread = stop.clone();
        let lock_path = path.clone();
        let handle = thread::spawn(move || {
            let mut last = Instant::now();
            loop {
                let elapsed = last.elapsed();

                if stop_thread.load(Ordering::SeqCst) {
                    break;
                }
                if elapsed >= interval {
                    last = Instant::now(); // сброс на текущее время
                    driver_thread.update_file_modified_time_now(&lock_path)?;
                }

                // Вместо thread::sleep() — park_timeout()
                let remaining = interval - elapsed;
                thread::park_timeout(remaining);
            }

            Ok(())
        });

        Ok(Self {
            uuid,
            driver,
            handle: Some(handle),
            stop,
            path,
        })
    }

    /// Проверка на блокировку файла
    #[instrument(level = "debug", skip(driver))]
    pub fn is_locked<P>(driver: Arc<D>, path: P) -> bool
    where
        P: AsRef<Path> + Debug,
    {
        // Если файл существует и он не старше 30 секунд, то блокировка установлена
        driver.exists(&path)
            && driver
                .stat(path)
                .map(|stat| stat.modified())
                .map(|modified| {
                    let time_exp = time_expired();
                    let expired = modified + time_exp;
                    let now = driver.time().unwrap_or_default();

                    debug!(?modified, ?time_exp, ?expired, ?now);
                    expired >= now
                })
                .unwrap_or(false)
    }
}

impl<D: Driver> Drop for BaseLock<D> {
    fn drop(&mut self) {
        debug!("Остановка потока обновления времени блокировки");
        self.stop.store(true, Ordering::SeqCst);

        debug!("Снятие блокировки");
        if let Err(err) = self.driver.rm(&self.path) {
            warn!(?err, "Ошибка при удалении файла блокировки");
        }

        debug!("Ожидание завершения потока блокировки");
        if let Some(handle) = self.handle.take() {
            handle.thread().unpark();
            if let Err(err) = handle.join() {
                warn!(?err, "Ошибка при завершении потока блокировки");
            }
        }

        debug!("Блокировка снята");
    }
}
