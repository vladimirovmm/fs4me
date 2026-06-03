use fs4me_interface::{Driver, DriverError, WriteMode};
use fs4me_uuid::FsUuid;
use std::{
    fmt::{Debug, Display},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tracing::{debug, instrument, warn};

use crate::helpers::{parent_dir_mast_exists, time_expired};

pub mod paths;
pub use crate::base_lock::paths::LockPaths;

/// Блокировка, предоставляющая эксклюзивный доступ к файлу и исключающая параллельное обращение к нему.
#[derive(Debug)]
pub struct BaseLock<D: Driver> {
    /// Уникальный идентификатор клиента.
    /// Используется для отображения в логах.
    uuid: FsUuid,
    /// Драйвер для работы с файловой системой.
    driver: Arc<D>,
    /// Путь до файла блокировки
    pub path: PathBuf,
    /// Путь до заблокированного lock файла
    pub block_path: PathBuf,
    /// Путь к временному файлу блокировки.
    ///
    /// В этот файл сначала записывается новое содержимое блокировки.
    /// После завершения записи файл атомарно перемещается на постоянное место
    /// основного файла блокировки.
    ///
    /// Такой подход снижает вероятность конфликтов и уменьшает задержки,
    /// возникающие при одновременном доступе нескольких процессов к файлу блокировки.
    pub tmp_path: PathBuf,
}

impl<D: Driver> Display for BaseLock<D> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{:?}", self.uuid, self.path)
    }
}

impl<'a, D: Driver> BaseLock<D> {
    pub fn try_form<P>(uuid: FsUuid, driver: Arc<D>, source_path: P) -> Result<Self, DriverError>
    where
        P: AsRef<Path> + Debug,
    {
        let LockPaths {
            path,
            block_path,
            tmp_path,
        } = source_path.as_ref().try_into()?;
        parent_dir_mast_exists(driver.clone(), &path)?;

        Ok(Self {
            uuid,
            driver,
            block_path,
            path,
            tmp_path,
        })
    }

    /// Проверяет, существует ли родительская директория для файла блокировки.
    ///
    /// @return Возвращает `Ok` в случае успеха, или `Err` в случае ошибки.
    pub fn parent_dir_mast_exists(&self) -> Result<(), DriverError> {
        parent_dir_mast_exists(self.driver.clone(), &self.path)
    }

    /// Блокирует файл блокировки, подготавливая его к последующей записи.
    ///
    /// Механизм блокировки основан на атомарной операции `mv` (перемещение),
    /// так как она гарантирует, что в любой момент времени заблокировать файл
    /// смогут только один процесс, независимо от типа файловой системы.
    ///
    /// Альтернативный подход `WriteMode::FailIfExists` не предоставляет таких
    /// гарантий для всех типов файловых систем и может приводить к race condition.
    ///
    /// @return Возвращается инстанс `Blocker`, который автоматически снимает блокировку
    /// при завершении работы (например, при падении процесса или при выходе из
    /// области видимости `Drop` реализации).
    #[instrument(level = "debug", skip(self))]
    pub fn try_lock(&'a self) -> Result<Blocker<'a, D>, DriverError> {
        self.parent_dir_mast_exists()?;

        if !self.driver.exists(&self.path) && !self.driver.exists(&self.block_path) {
            debug!(?self.uuid, "Файл блокировки не существует, создаём его");
            let mut writer = self.driver.write(&self.path, WriteMode::FailIfExists)?;
            write!(writer, "").map_err(|err| DriverError::WriteError {
                path: self.path.clone(),
                reason: err.to_string(),
            })?;
            writer.flush().map_err(|err| DriverError::WriteError {
                path: self.path.clone(),
                reason: err.to_string(),
            })?;
        }

        if self.is_locked() {
            return Err(DriverError::LockFileBLocked(self.block_path.clone()));
        }

        if self.driver.exists(&self.block_path) {
            // Такое случается, если блокировка чужая продержалась более 30 секунд
            return Ok(Blocker(self));
        }
        self.driver
            .mv(&self.path, &self.block_path)
            .inspect(|_| debug!(?self.uuid, "Успешная блокировка файла {:?}", self.path))
            .inspect_err(|err| debug!(?self.uuid, "Файл блокировки уже занят {err}"))
            .map(|_| Blocker(self))
    }

    /// Проверка на существование lock-файла
    #[instrument(level = "debug", skip(self))]
    pub fn exist(&self) -> bool {
        self.driver.exists(&self.path) || self.driver.exists(&self.block_path)
    }

    /// Проверка на блокировку файла
    #[instrument(level = "debug", skip(self))]
    pub fn is_locked(&self) -> bool {
        // Если файл существует и он не старше 30 секунд, то блокировка установлена
        self.driver.exists(&self.block_path)
            && self
                .driver
                .stat(&self.block_path)
                .map(|stat| {
                    stat.modified() + Duration::from_secs(time_expired())
                        >= self.driver.time().unwrap_or_default()
                })
                .unwrap_or(false)
    }

    /// Снятия блокировки
    #[instrument(level = "debug", skip(self))]
    pub fn unlock(&self) -> Result<(), DriverError> {
        debug!(?self.uuid, "Разблокировка Lock-файла");
        if self.driver.exists(&self.tmp_path) {
            self.driver.mv(&self.tmp_path, &self.path)?;
            if self.driver.exists(&self.block_path) {
                self.driver.rm(&self.block_path)?;
            }
        } else if self.driver.exists(&self.block_path) {
            self.driver.mv(&self.block_path, &self.path)?;
        }

        Ok(())
    }

    /// @return Путь до файла блокировки
    pub fn path(&self) -> &Path {
        if self.driver.exists(&self.block_path) {
            return &self.block_path;
        }
        &self.path
    }
}

pub struct Blocker<'a, D: Driver>(pub(crate) &'a BaseLock<D>);

impl<'a, D: Driver> Drop for Blocker<'a, D> {
    fn drop(&mut self) {
        if let Err(err) = self.0.unlock() {
            let path = &self.0.path;
            warn!(?path, ?err, "Ошибка при разблокировки");
        }
    }
}
