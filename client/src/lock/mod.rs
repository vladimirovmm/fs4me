use fs4me_interface::{Driver, DriverError, WriteMode};
use std::{
    fmt::{Debug, Display},
    io,
    path::{Path, PathBuf},
    str::FromStr,
    thread::sleep,
    time::{Duration, Instant},
};
use tracing::{debug, error, instrument, warn};

pub(crate) mod lock_info;
pub mod lock_path;

use crate::lock::lock_info::{LockInfo, LockInfoRead};
use crate::{Fs, lock::lock_path::LockPath};

/// Возвращает родительскую директорию для указанного пути.
///
/// @param path Путь к файлу/директории.
/// @returns Путь к родительской директории.
pub fn parent_dir(path: &Path) -> Result<&Path, DriverError> {
    path.parent()
        .ok_or_else(|| DriverError::ParentDirError(path.to_path_buf()))
}

/// Проверяет, существует ли родительская директория для указанного пути.
///
/// @return Возвращает `Ok` в случае успеха, или `Err` в случае ошибки.
#[instrument(level = "debug", skip(fs))]
fn parent_dir_mast_exists<D, P>(fs: &Fs<D>, path: P) -> Result<(), DriverError>
where
    D: Driver,
    P: AsRef<Path> + Debug,
{
    let path = path.as_ref();
    parent_dir(path).and_then(|path| {
        if fs.exists(path) {
            debug!("Родительская директория существует: {path:?}");
            Ok(())
        } else {
            warn!("Родительская директория не существует: {path:?}");
            Err(DriverError::ParentDirError(path.to_path_buf()))
        }
    })
}

/// Функция для повторения попыток блокировки и разблокировки
/// Время на для повторения 30 секунд
///
/// @param `retry_fn` - функция, которая будет повторяться.
/// @returns результат повторений.
#[instrument(level = "debug", skip_all)]
fn retry<F>(mut retry_fn: F) -> Result<(), DriverError>
where
    F: FnMut() -> Result<(), DriverError>,
{
    // Время начала. От этого момента будет отсчитываться 30 секунд
    let start = Instant::now();
    // Интервал между повторами
    let mut interval = Duration::from_millis(50);
    // Максимальное время для повторений. 30 секунд.
    let limit_secs = Duration::from_secs(30);

    loop {
        let result = retry_fn();
        debug!(?result);

        // При данных ошибках не повторять попытку
        // Причины прекратить повторение:
        // - При отсутствии родительской директории
        // - Если не удалось получить имя файла
        if let Err(err) = &result
            && matches!(
                err,
                DriverError::ParentDirError(..) | DriverError::FileNameError(..)
            )
        {
            return result;
        }

        // Либо успех, либо время вышло
        if result.is_ok() || start.elapsed() > limit_secs {
            return result;
        }

        if interval.as_secs() < 3 {
            interval *= 2;
        } else {
            interval = Duration::from_secs(1);
        }

        let jitter = Duration::from_millis(rand::random_range(0..250));
        sleep(interval + jitter);
    }
}

pub struct MultiLock<'a, D: Driver> {
    /// Клиент для работы с файловой системой.
    fs: &'a Fs<D>,
    /// Файл или директория, к которую нужно заблокировать.
    source_path: PathBuf,
    /// Хеш содержимого блокировки.
    hash: Option<u64>,
    /// Время последнего изменения блокировки.
    modified_time: Option<Duration>,
    /// Для работы с путями файла блокировки
    lock_path: LockPath<'a, D>,
}

impl<D: Driver> Display for MultiLock<'_, D> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Lock - uuid: {}, path: {:?}",
            self.fs.uuid, self.source_path
        )
    }
}

impl<'a, D: Driver> MultiLock<'a, D> {
    /// Блокирует файл или директорию для чтения или записи.
    ///
    /// @param fs - Клиент, к которой подключен драйвер.
    /// @param path - Путь к файлу или директории.
    /// @param mode - Режим блокировки.
    /// @return Возвращает `Ok` с блокировкой в случае успеха, или `Err` в случае ошибки.
    #[instrument(level = "debug", skip(fs))]
    pub fn try_from<P>(fs: &'a Fs<D>, path: P, mode: LockMode) -> Result<Self, DriverError>
    where
        P: AsRef<Path> + Debug,
    {
        let source_path = path.as_ref().to_path_buf();
        let mut lock = Self {
            lock_path: LockPath::try_form(fs, &source_path)?,
            fs,
            source_path,
            hash: None,
            modified_time: None,
        };

        lock.retry_lock(mode)?;
        Ok(lock)
    }

    /// Возвращает информацию о блокировке файла или директории.
    ///
    /// @return Возвращает `Ok` с информацией о блокировке, или `Err` в случае ошибки.
    #[instrument(level = "debug", skip(self))]
    fn read(&self) -> Result<LockInfoRead, DriverError> {
        if !self.lock_path.exist() {
            debug!("Блокировка не существует, возвращаем пустую информацию");
            // Если lock файл не существует, возвращаем пустую структуру LockStat
            return Ok(LockInfoRead::default());
        }

        let path = &self.lock_path.block_path;

        debug!("Читаем файл блокировки");
        // Читаем содержимое lock файла
        let lock_reader = self.fs.driver.read(path, 0)?;
        let lock_content =
            io::read_to_string(lock_reader).map_err(|err| DriverError::ReadSeekError {
                path: self.lock_path.path.clone(),
                reason: err.to_string(),
            })?;

        // Парсим содержимое lock файла в структуру LockStat
        let mut lock_info = LockInfo::from_str(&lock_content)?;
        // Удаляем устаревшие блокировки (unixtime + 5 минут < now)
        lock_info.remove_stale(self.fs.time()?);

        // Вычисляем хеш содержимого lock файла
        let hash = Some(lock_info.get_hash());
        // Получаем время последнего изменения lock файла
        let modified_time = self.fs.stat(path).ok().map(|stat| stat.modified());
        if modified_time.is_none() {
            // Если lock файл не существует, возвращаем пустую структуру LockStat
            return Ok(LockInfoRead::default());
        }

        Ok(LockInfoRead {
            lock_info,
            modified_time,
            hash,
        })
    }

    /// Используется для метода write, если нужно записать новое содержимое
    #[instrument(level = "debug", skip(self))]
    fn write_from_replace(&self, lock: LockInfo) -> Result<(), DriverError> {
        let tmp_path = &self.lock_path.tmp_path;

        debug!(?self.fs.uuid, ?tmp_path, ?lock, "Пишем во временный файл блокировки");
        // Записываем строку в lock файл
        let mut lock_writer = self.fs.driver.write(&tmp_path, WriteMode::Overwrite)?;
        lock_writer
            .write_all(lock.to_string().as_bytes())
            .map_err(|err| DriverError::WriteError {
                path: tmp_path.clone(),
                reason: err.to_string(),
            })?;
        lock_writer.flush().map_err(|err| DriverError::WriteError {
            path: tmp_path.to_path_buf(),
            reason: err.to_string(),
        })?;
        drop(lock_writer);

        debug!(
            "После выхода из области видимости файл блокировки должен быть заменён. См. LockPath"
        );

        Ok(())
    }

    /// Используется для метода write, если содержимое блокировки пустое.
    /// Это означает, что все блокировки сняты, и файл блокировки больше не нужен.
    #[instrument(level = "debug", skip_all)]
    fn drop_lock(&self) -> Result<(), DriverError> {
        debug!("Удаляем файл блокировки потому что он пустой");
        // Удаляем файл блокировки
        self.fs.driver.rm(&self.lock_path.block_path)
    }

    /// Записывает информацию о блокировке файла или директории.
    ///
    /// @param lock - Информация о блокировке.
    /// @return Возвращает `Ok` в случае успеха, или `Err` в случае ошибки.
    #[instrument(level = "debug", skip_all)]
    fn write(&self, lock: LockInfo) -> Result<(), DriverError> {
        if lock.is_empty() {
            // Блокировок нет больше, удаляем файл блокировки
            return self.drop_lock();
        }
        // Обновляем запись о блокировках
        self.write_from_replace(lock)
    }

    /// Пытается блокировать файл/директорию для чтения/записи.
    ///
    /// @param mode - Режим блокировки: `Read`, `Write` и `WriteQueue`.
    /// @return Result<()> - Результат: успех или ошибка
    #[instrument(level = "debug", skip(self))]
    fn try_lock(&mut self, mode: LockMode) -> Result<(), DriverError> {
        if matches!(mode, LockMode::Write) {
            debug!("Перед блокировкой на запись нужно встать в очередь");
            self.try_lock(LockMode::WriteQueue)?;
        }

        // Создаём уникальный доступ к lock файлу

        let _lock = self.lock_path.try_lock()?;
        debug!(?self.fs.uuid,"Установлена блокировка");

        let LockInfoRead {
            mut lock_info,
            modified_time,
            hash,
        } = self.read()?;
        self.hash = hash;
        self.modified_time = modified_time;

        debug!(?lock_info, "ДО");
        lock_info
            .set(self.fs.uuid, self.fs.time()?, mode)
            .inspect_err(|err| debug!(?self.fs.uuid, ?err))
            .map_err(|_| DriverError::LockedError {
                path: self.source_path.clone(),
                mode: mode.to_string(),
            })?;
        debug!(?lock_info, "ПОСЛЕ");

        self.write(lock_info)
    }

    /// Попытка снять блокировку от имени текущего uuid.
    ///
    /// @return Result<()> - Результат: успех или ошибка
    #[instrument(level = "debug", skip(self))]
    fn try_unlock(&mut self) -> Result<(), DriverError> {
        // Создаём уникальный доступ к lock файлу
        let _lock = self.lock_path.try_lock()?;

        let LockInfoRead {
            mut lock_info,
            modified_time,
            hash,
        } = self.read()?;
        self.hash = hash;
        self.modified_time = modified_time;

        debug!(?self.fs.uuid, "Убираем uuid из списка блокировки");
        lock_info.remove(self.fs);
        self.write(lock_info)
    }

    /// Попытка блокировки файла/директории для чтения/записи в течение 30 секунд.
    /// При неудаче используется стратегия Backoff
    ///
    /// @param path - Путь к файлу или директории.
    /// @param mode - Режим блокировки: `Read`, `Write` и `WriteQueue`.
    /// @return Result<()> - Результат: успех или ошибка
    #[instrument(level = "debug", skip(self))]
    fn retry_lock(&mut self, mode: LockMode) -> Result<(), DriverError> {
        retry(|| -> Result<(), DriverError> {
            // Максимальное время ожидания
            self.try_lock(mode)
        })
    }

    /// Снять блокировку от имени текущего uuid.
    /// При неудаче используется стратегия Backoff
    ///
    /// @return Result<()> - Результат: успех или ошибка
    #[instrument(level = "debug", skip(self))]
    fn retry_unlock(&mut self) -> Result<(), DriverError> {
        retry(|| -> Result<(), DriverError> { self.try_unlock() })
    }
}

impl<'a, D: Driver> Drop for MultiLock<'a, D> {
    fn drop(&mut self) {
        if let Err(e) = self.retry_unlock() {
            error!("Ошибка при снятии блокировки: {e}. {self}");
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockMode {
    Read,
    Write,
    WriteQueue,
}

impl FromStr for LockMode {
    type Err = DriverError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "read" => Ok(LockMode::Read),
            "write" => Ok(LockMode::Write),
            "write_queue" => Ok(LockMode::WriteQueue),
            _ => Err(DriverError::ParseLockError {
                reason: format!("Некорректный формат режима блокировки: {s}"),
            }),
        }
    }
}

impl Display for LockMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LockMode::Read => write!(f, "read"),
            LockMode::Write => write!(f, "write"),
            LockMode::WriteQueue => write!(f, "write_queue"),
        }
    }
}
