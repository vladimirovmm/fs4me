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
    time::Instant,
};
use tracing::{debug, instrument, warn};

use crate::{
    base_lock::paths::base_lock_path,
    helpers::{background_refresh_interval, parent_dir_mast_exists, time_expired},
};

pub mod paths;

/// Блокировка, предоставляющая эксклюзивный доступ к файлу и исключающая параллельное обращение к нему.
#[derive(Debug)]
pub struct BaseLock<D: Driver> {
    /// Уникальный идентификатор клиента.
    /// Используется для отображения в логах.
    uuid: FsUuid,
    /// Драйвер для работы с файловой системой.
    driver: Arc<D>,
    /// Поток, который обновляет время блокировки.
    handle: Option<JoinHandle<Result<(), DriverError>>>,
    /// Указывает, была ли создана блокировка.
    /// От неё зависит, нужно ли удалять файл блокировки и останавливать поток обновления.
    blocked: Arc<AtomicBool>,
    /// Путь до файла блокировки
    path: PathBuf,
}

impl<D: Driver> Display for BaseLock<D> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{:?}", self.uuid, self.path)
    }
}

impl<D: Driver> BaseLock<D> {
    /// Пытаемся получить блокировку для указанного пути.
    fn try_reserved(self) -> Result<Self, DriverError> {
        parent_dir_mast_exists(self.driver.clone(), &self.path)?;

        debug!(?self.uuid, ?self.path, "Попытка создать файл блокировки");

        // Пытаемся создать файл блокировки
        let result = || -> Result<(), DriverError> {
            let mut writer = self.driver.write(&self.path, WriteMode::FailIfExist)?;
            write!(writer, "{}", self.uuid).map_err(|err| DriverError::WriteError {
                path: self.path.clone(),
                reason: err.to_string(),
            })?;
            writer.flush().map_err(|err| DriverError::WriteError {
                path: self.path.clone(),
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
            if Self::is_locked(self.driver.clone(), &self.path) {
                debug!("Файл заблокирован");
                return Err(err);
            }
            debug!("Удаление файла блокировки, потому что она не действительна");
            // Удаляем файл блокировки, если он существует. Причина блокировки считается неактуальной.
            // Может возникнуть ошибка, если файл уже был удален другим процессом.
            // Тогда не повторяем, потому что кто-то уже получил блокировку.
            self.driver.rm(&self.path)?;
            // Пытаемся получить блокировку заново
            return self.try_reserved();
        }

        debug!(
            "Чтение содержимого файла блокировки, чтобы проверить принадлежность текущему клиенту"
        );
        let content = self.driver.read_all_string(&self.path)?;
        if content.trim() != self.uuid.to_string() {
            debug!("Файл блокировки заблокирован другим клиентом");
            return Err(DriverError::LockFileBLocked(self.path.clone()));
        }

        // Блокировка успешно установлена, теперь требуется очистка и остановка потока обновления.
        self.blocked.store(true, Ordering::SeqCst);

        Ok(self)
    }

    /// Регулярно обновляет время обновления файла блокировки, поддерживая актуальность блокировки.
    ///
    /// @returns `JoinHandle` для управления потоком обновления.
    fn background_lock_refresh(&self) -> JoinHandle<Result<(), DriverError>> {
        debug!("Инициализация потока обновления времени блокировки");
        let interval_thread = background_refresh_interval();
        let driver_thread = self.driver.clone();
        let blocked_thread = self.blocked.clone();
        let path_thread = self.path.clone();
        thread::spawn(move || {
            let mut last = Instant::now();
            debug!(
                ?interval_thread,
                ?path_thread,
                "Поток обновления времени блокировки запущен"
            );
            loop {
                debug!("loop");
                if !blocked_thread.load(Ordering::SeqCst) {
                    debug!("Обновление времени блокировки остановлено");
                    break;
                }

                let elapsed = last.elapsed();
                if elapsed >= interval_thread {
                    debug!(?elapsed, "Обновление времени блокировки");
                    driver_thread.update_file_modified_time_now(&path_thread)?;
                    debug!("123");
                    last = Instant::now(); // сброс на текущее время
                }

                // Вместо thread::sleep() — park_timeout()
                let remaining = interval_thread.saturating_sub(elapsed);
                thread::park_timeout(remaining);
            }

            Ok::<(), DriverError>(())
        })
    }

    /// Пытаемся создать блокировку для указанного пути и заблокировать файл блокировки.
    #[instrument(level = "debug", skip(driver))]
    pub fn try_lock<P>(uuid: FsUuid, driver: Arc<D>, source_path: P) -> Result<Self, DriverError>
    where
        P: AsRef<Path> + Debug,
    {
        let lock = Self {
            uuid,
            driver,
            handle: None,
            // Пока ещё не удалось установить блокировку.
            // После успешного блокирования будет изменено на true.
            blocked: Arc::new(AtomicBool::new(false)),
            path: base_lock_path(&source_path)?,
        };

        // Пытаемся заблокировать файл блокировки
        let mut lock = lock.try_reserved()?;

        // Поток, который периодически обновляет время блокировки
        lock.handle = Some(lock.background_lock_refresh());

        Ok(lock)
    }

    /// Проверка на блокировку файла
    #[instrument(level = "debug", skip(driver))]
    pub fn is_locked<P>(driver: Arc<D>, path: P) -> bool
    where
        P: AsRef<Path> + Debug,
    {
        // Если файл существует и он не старше 5 минут, то блокировка установлена
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
        let blocked = self.blocked.load(Ordering::SeqCst);
        if !blocked {
            // Это может быть если блокировка не удалась
            debug!("Поток обновления времени блокировки уже остановлен");
            return;
        }
        debug!("Остановка потока обновления времени блокировки");
        self.blocked.store(false, Ordering::SeqCst);

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
