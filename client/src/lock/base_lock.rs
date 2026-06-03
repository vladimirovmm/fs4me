use fs4me_interface::{Driver, DriverError, WriteMode};
use rand::{RngExt, distr::Alphanumeric};
use std::{
    fmt::Debug,
    path::{Path, PathBuf},
    time::Duration,
};
use tracing::{debug, instrument, warn};

use crate::{
    Fs,
    lock::{parent_dir, parent_dir_mast_exists},
};

/// Блокировка, предоставляющая эксклюзивный доступ к файлу и исключающая параллельное обращение к нему.
#[derive(Debug)]
pub struct BaseLock<'a, D: Driver> {
    /// Клиент для работы с файловой системой.
    fs: &'a Fs<D>,
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

impl<'a, D: Driver> BaseLock<'a, D> {
    pub fn try_form<P>(fs: &'a Fs<D>, source_path: P) -> Result<Self, DriverError>
    where
        P: AsRef<Path> + Debug,
    {
        let source_path = source_path.as_ref();
        parent_dir_mast_exists(fs, source_path)?;

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

        let lock_path = parent.join(&lock_file_name);
        let block_lock_path = parent.join(format!("~{lock_file_name}"));

        let mut rng = rand::rng();
        let tmp_lock_path = parent.join(format!(
            "~{lock_file_name}.{}",
            (0..9)
                .map(|_| rng.sample(Alphanumeric) as char)
                .collect::<String>()
        ));

        Ok(Self {
            fs,
            block_path: block_lock_path,
            path: lock_path,
            tmp_path: tmp_lock_path,
        })
    }

    /// Проверяет, существует ли родительская директория для файла блокировки.
    ///
    /// @return Возвращает `Ok` в случае успеха, или `Err` в случае ошибки.
    pub fn parent_dir_mast_exists(&self) -> Result<(), DriverError> {
        parent_dir_mast_exists(self.fs, &self.path)
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

        if !self.fs.exists(&self.path) && !self.fs.exists(&self.block_path) {
            debug!(?self.fs.uuid, "Файл блокировки не существует, создаём его");
            let mut writer = self.fs.driver.write(&self.path, WriteMode::FailIfExists)?;
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

        if self.fs.exists(&self.block_path) {
            // Такое случается, если блокировка чужая продержалась более 30 секунд
            return Ok(Blocker(self));
        }
        self.fs
            .driver
            .mv(&self.path, &self.block_path)
            .inspect(|_| debug!(?self.fs.uuid, "Успешная блокировка файла {:?}", self.path))
            .inspect_err(|err| debug!(?self.fs.uuid, "Файл блокировки уже занят {err}"))
            .map(|_| Blocker(self))
    }

    /// Проверка на существование lock-файла
    #[instrument(level = "debug", skip(self))]
    pub fn exist(&self) -> bool {
        self.fs.exists(&self.path) || self.fs.exists(&self.block_path)
    }

    /// Проверка на блокировку файла
    #[instrument(level = "debug", skip(self))]
    pub fn is_locked(&self) -> bool {
        // Если файл существует и он не старше 30 секунд, то блокировка установлена
        self.fs.exists(&self.block_path)
            && self
                .fs
                .stat(&self.block_path)
                .map(|stat| {
                    stat.modified() + Duration::from_secs(30) >= self.fs.time().unwrap_or_default()
                })
                .unwrap_or(false)
    }

    /// Снятия блокировки
    #[instrument(level = "debug", skip(self))]
    pub fn unlock(&self) -> Result<(), DriverError> {
        debug!(?self.fs.uuid, "Разблокировка Lock-файла");
        if self.fs.exists(&self.tmp_path) {
            self.fs.driver.mv(&self.tmp_path, &self.path)?;
            if self.fs.exists(&self.block_path) {
                self.fs.driver.rm(&self.block_path)?;
            }
        } else if self.fs.exists(&self.block_path) {
            self.fs.driver.mv(&self.block_path, &self.path)?;
        }

        Ok(())
    }

    /// @return Путь до файла блокировки
    pub fn path(&self) -> &Path {
        if self.fs.driver.exists(&self.block_path) {
            return &self.block_path;
        }
        &self.path
    }
}

pub struct Blocker<'a, D: Driver>(pub(crate) &'a BaseLock<'a, D>);

impl<'a, D: Driver> Drop for Blocker<'a, D> {
    fn drop(&mut self) {
        if let Err(err) = self.0.unlock() {
            let path = &self.0.path;
            warn!(?path, ?err, "Ошибка при разблокировки");
        }
    }
}
