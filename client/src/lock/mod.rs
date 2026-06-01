use fs4me_interface::{Driver, DriverError, WriteMode};
use rand::{RngExt, distr::Alphanumeric};
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

use crate::Fs;
use crate::lock::lock_info::{LockInfo, LockInfoRead};

/// Возвращает родительскую директорию для указанного пути.
///
/// @param path Путь к файлу/директории.
/// @returns Путь к родительской директории.
pub fn parent_dir(path: &Path) -> Result<&Path, DriverError> {
    path.parent()
        .ok_or_else(|| DriverError::ParentDirError(path.to_path_buf()))
}

/// Получить путь к файлу блокировки для указанного пути.
///
/// @param path Путь к файлу.
/// @returns Путь к файлу блокировки.
pub fn path_to_lock_file<P: AsRef<Path>>(path: P) -> Result<PathBuf, DriverError> {
    let path = path.as_ref();
    let parent = parent_dir(path)?;
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

/// Получить путь к временному файлу блокировки для указанного пути.
///
/// @param path Путь к файлу.
/// @returns Путь к временному файлу блокировки.
pub fn tmp_lock_path<P: AsRef<Path>>(path: P) -> Result<PathBuf, DriverError> {
    let mut path = path_to_lock_file(path)?;
    let mut rng = rand::rng();
    path.set_file_name(format!(
        "{}.{}",
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default(),
        (0..9)
            .map(|_| rng.sample(Alphanumeric) as char)
            .collect::<String>()
    ));
    Ok(path)
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
    let mut interval = Duration::from_millis(100);
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

        if interval.as_secs_f64() < 3.0 {
            interval *= 2;
        } else {
            interval = Duration::from_secs(1);
        }

        let jitter = Duration::from_millis(rand::random_range(0..250));
        sleep(interval + jitter);
    }
}

pub struct Lock<'a, D: Driver> {
    /// Клиент для работы с файловой системой.
    fs: &'a Fs<D>,
    /// Файл или директория, к которую нужно заблокировать.
    source_path: PathBuf,
    /// Хеш содержимого блокировки.
    hash: Option<u64>,
    /// Время последнего изменения блокировки.
    modified_time: Option<u32>,
}

impl<D: Driver> Display for Lock<'_, D> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Lock - uuid: {}, path: {:?}",
            self.fs.uuid, self.source_path
        )
    }
}

impl<'a, D: Driver> Lock<'a, D> {
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
        let path = path.as_ref();
        let mut lock = Self {
            fs,
            source_path: path.to_path_buf(),
            hash: None,
            modified_time: None,
        };

        lock.retry_lock(mode)?;
        Ok(lock)
    }

    /// Проверяет, существует ли родительская директория для указанного пути.
    ///
    /// @return Возвращает `Ok` в случае успеха, или `Err` в случае ошибки.
    #[instrument(level = "debug", skip(self))]
    fn parent_dir_mast_exists(&self) -> Result<(), DriverError> {
        parent_dir(&self.source_path).and_then(|path| {
            if self.fs.exists(path) {
                debug!("Родительская директория существует: {path:?}");
                Ok(())
            } else {
                warn!("Родительская директория не существует: {path:?}");
                Err(DriverError::ParentDirError(self.source_path.clone()))
            }
        })
    }

    /// Возвращает информацию о блокировке файла или директории.
    ///
    /// @return Возвращает `Ok` с информацией о блокировке, или `Err` в случае ошибки.
    #[instrument(level = "debug", skip(self))]
    fn read(&self) -> Result<LockInfoRead, DriverError> {
        let lock_file = path_to_lock_file(&self.source_path)?;
        debug!(?lock_file);

        if !self.fs.exists(&lock_file) {
            debug!(?lock_file, "Блокировки не существует");
            // Если lock файл не существует, возвращаем пустую структуру LockStat
            return Ok(LockInfoRead::default());
        }

        debug!(?lock_file, "Читаем файл блокировки");
        // Читаем содержимое lock файла
        let lock_reader = self.fs.driver.read(&lock_file, 0)?;
        let lock_content =
            io::read_to_string(lock_reader).map_err(|err| DriverError::ReadSeekError {
                path: lock_file.to_path_buf(),
                reason: err.to_string(),
            })?;
        // Парсим содержимое lock файла в структуру LockStat
        let mut lock_info = LockInfo::from_str(&lock_content)?;
        // Удаляем устаревшие блокировки (unixtime + 5 минут < now)
        lock_info.remove_stale(self.fs.time()?);

        // Вычисляем хеш содержимого lock файла
        let hash = Some(lock_info.get_hash());
        // Получаем время последнего изменения lock файла
        let modified_time = self.fs.stat(&lock_file).ok().map(|stat| stat.modified());
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
    fn write_from_replace(&self, tmp_path: &Path, lock_content: String) -> Result<(), DriverError> {
        // Записываем строку в lock файл
        let mut lock_writer = self.fs.driver.write(&tmp_path, WriteMode::FailIfExists)?;
        lock_writer
            .write_all(lock_content.as_bytes())
            .map_err(|err| DriverError::WriteError {
                path: tmp_path.to_path_buf(),
                reason: err.to_string(),
            })?;
        lock_writer.flush().map_err(|err| DriverError::WriteError {
            path: tmp_path.to_path_buf(),
            reason: err.to_string(),
        })?;
        drop(lock_writer);

        let path = path_to_lock_file(&self.source_path)?;
        let LockInfoRead {
            modified_time,
            hash,
            ..
        } = self.read()?;
        // Убеждаемся, что блокировка не была изменена другими клиентами
        if self.hash != hash || self.modified_time != modified_time {
            return Err(DriverError::LockChangedError(path));
        }
        // Перемещаем временный файл в окончательное место
        self.fs.driver.mv(tmp_path, &path)
    }

    /// Используется для метода write, если содержимое блокировки пустое.
    /// Это означает, что все блокировки сняты, и файл блокировки больше не нужен.
    #[instrument(level = "debug", skip_all)]
    fn drop_lock(&self) -> Result<(), DriverError> {
        let lock_path = path_to_lock_file(&self.source_path)?;
        if !self.fs.exists(&lock_path) {
            // Файл блокировки больше не существует, удалять его не нужно
            warn!(?lock_path, "Попытка снятия несуществующей блокировки");
            return Ok(());
        }
        let LockInfoRead {
            modified_time,
            hash,
            ..
        } = self.read()?;
        // Убеждаемся, что блокировка не была изменена другими клиентами
        if self.hash != hash || self.modified_time != modified_time {
            return Err(DriverError::LockChangedError(lock_path));
        }
        // Удаляем файл блокировки
        self.fs.driver.rm(&lock_path)
    }

    /// Записывает информацию о блокировке файла или директории.
    ///
    /// @param lock - Информация о блокировке.
    /// @return Возвращает `Ok` в случае успеха, или `Err` в случае ошибки.
    fn write(&self, lock: LockInfo) -> Result<(), DriverError> {
        if lock.is_empty() {
            // Блокировок нет больше, удаляем файл блокировки
            return self.drop_lock();
        }
        // Путь до временного файла блокировки.
        // Данные будут записаны в этот файл, после чего он будет атомарно перемещён на место основного файла блокировки.
        let tmp_path = tmp_lock_path(&self.source_path)?;

        // Преобразуем структуру LockStat в строку
        let lock_content = lock.to_string();

        self.write_from_replace(&tmp_path, lock_content).map_err(|err| {
            // Удаляем временный файл в случае ошибки
            if self.fs.exists(&tmp_path) && let Err(err_rm) = self.fs.rm(tmp_path) {
                error!("Ошибка при удалении временного файла блокировки: {err_rm}. Причина удаления временного файла: {err}");
            }
            err
        })
    }

    /// Пытается блокировать файл/директорию для чтения/записи.
    ///
    /// @param mode - Режим блокировки: `Read`, `Write` и `WriteQueue`.
    /// @return Result<()> - Результат: успех или ошибка
    #[instrument(level = "debug", skip(self))]
    fn try_lock(&mut self, mode: LockMode) -> Result<(), DriverError> {
        self.parent_dir_mast_exists()?;

        if matches!(mode, LockMode::Write) {
            debug!("Перед блокировкой на запись нужно встать в очередь");
            self.try_lock(LockMode::WriteQueue)?;
        }

        let LockInfoRead {
            mut lock_info,
            modified_time,
            hash,
        } = self.read()?;
        self.hash = hash;
        self.modified_time = modified_time;

        lock_info
            .set(self.fs.uuid, self.fs.time()?, mode)
            .map_err(|_| DriverError::LockedError {
                path: self.source_path.clone(),
                mode: mode.to_string(),
            })?;
        self.write(lock_info)
    }

    /// Попытка снять блокировку от имени текущего uuid.
    ///
    /// @return Result<()> - Результат: успех или ошибка
    #[instrument(level = "debug", skip(self))]
    fn try_unlock(&mut self) -> Result<(), DriverError> {
        self.parent_dir_mast_exists()?;

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

impl<'a, D: Driver> Drop for Lock<'a, D> {
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

#[cfg(test)]
mod tests {

    use super::path_to_lock_file;
    use std::path::PathBuf;
    use tracing_test::traced_test;

    #[test]
    #[traced_test]
    fn test_lock_path() {
        for (path, expected) in [
            ("a/b/c.txt", "a/b/.c.txt.lock"),
            ("a/b/.c.txt", "a/b/.c.txt.lock"),
            ("a/b/txt", "a/b/.txt.lock"),
        ] {
            let lock_path = path_to_lock_file(path).unwrap();
            assert_eq!(lock_path, PathBuf::from(expected));
        }
    }
}
