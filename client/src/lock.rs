use fs4me_interface::{Driver, DriverError, WriteMode};
use rand::{RngExt, distr::Alphanumeric};
use std::{
    collections::VecDeque,
    fmt::{Debug, Display},
    hash::{DefaultHasher, Hash, Hasher},
    io,
    path::{Path, PathBuf},
    str::FromStr,
    thread::sleep,
    time::{Duration, Instant},
};
use tracing::{debug, error, instrument, warn};

use crate::{Fs, uuid::FsUuid};

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
pub fn lock_path<P: AsRef<Path>>(path: P) -> Result<PathBuf, DriverError> {
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
    let mut path = lock_path(path)?;
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
        let lock_file = lock_path(&self.source_path)?;
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

    /// Записывает информацию о блокировке файла или директории.
    ///
    /// @param lock - Информация о блокировке.
    /// @return Возвращает `Ok` в случае успеха, или `Err` в случае ошибки.
    fn write(&self, lock: LockInfo) -> Result<(), DriverError> {
        // Создаем временный файл для записи блокировки
        let tmp_path = tmp_lock_path(&self.source_path)?;
        // Преобразуем структуру LockStat в строку
        let lock_content = lock.to_string();

        // Перемещаем временный файл в окончательное место
        (|| {
	        // Записываем строку в lock файл
	        let mut lock_writer = self.fs.driver.write(&tmp_path, WriteMode::FailIfExists)?;
	        lock_writer
	            .write_all(lock_content.as_bytes())
	            .map_err(|err| DriverError::WriteError {
	                path: tmp_path.clone(),
	                reason: err.to_string(),
	            })?;
	        lock_writer.flush().map_err(|err| DriverError::WriteError {
	            path: tmp_path.clone(),
	            reason: err.to_string(),
	        })?;
	        drop(lock_writer);

            let path = lock_path(&self.source_path)?;
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
            self.fs.driver.mv(&tmp_path, &path)
        })()
        .map_err(|err| {
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
        self.parent_dir_mast_exists()?;

        // Время начала. От этого момента будет отсчитываться 30 секунд
        let start = Instant::now();
        // Интервал между повторами
        let mut interval = Duration::from_millis(100);

        loop {
            let result = self.try_lock(mode);
            debug!(?result);

            // Либо успех, либо время вышло
            if result.is_ok() || start.elapsed() > Duration::from_secs(30) {
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

    /// Снять блокировку от имени текущего uuid.
    /// При неудаче используется стратегия Backoff
    ///
    /// @return Result<()> - Результат: успех или ошибка
    #[instrument(level = "debug", skip(self))]
    fn retry_unlock(&mut self) -> Result<(), DriverError> {
        self.parent_dir_mast_exists()?;

        // Время начала. От этого момента будет отсчитываться 30 секунд
        let start = Instant::now();
        // Интервал между повторами
        let mut interval = Duration::from_millis(100);

        loop {
            let result = self.try_unlock();
            // Либо успех, либо время вышло
            if result.is_ok() || start.elapsed() > Duration::from_secs(30) {
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

/// Структура, содержащая информацию о блокировке файла и её статусе.
#[derive(Debug, Default)]
struct LockInfoRead {
    /// Информация о блокировке файла.
    lock_info: LockInfo,
    /// Время последнего изменения файла.
    modified_time: Option<u32>,
    /// Хеш файла.
    hash: Option<u64>,
}

/// Информация о блокировке файла.
/// Хранит информацию о читателях, писателях и очереди на запись.
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
pub(crate) struct LockInfo {
    /// Карта читателей: uuid -> unixtime блокировки
    read: Vec<(FsUuid, u32)>,
    /// Карта писателей: uuid -> unixtime блокировки
    write: Option<(FsUuid, u32)>,
    /// Очередь на запись: список uuid ожидающих блокировки.
    write_queue: VecDeque<(FsUuid, u32)>,
}

/// Преобразует текс в структуру `LockStat`.
///
/// Ожидается содержимое текста блокировки.
/// Каждая строка в файле — это отдельный читатель или писатель.
///
/// Формат строки:
/// FsUuid=unixtime=mode
///
/// Пример файла:
/// 12345_1=1620000000=read
/// 23456_2=1620000000=read
/// 34567_1=1620000000=write
/// 45678_1=1620000000=write_queue
impl FromStr for LockInfo {
    type Err = DriverError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut stat = LockInfo::default();
        for line in s.lines() {
            let Some((uuid, unixtime, mode)) = LockInfo::parse_line(line)? else {
                continue;
            };
            match mode {
                // Читают файл
                LockMode::Read => {
                    stat.read.push((uuid, unixtime));
                }
                // Заблокирован на запись
                LockMode::Write => {
                    stat.write = Some((uuid, unixtime));
                }
                // Очередь на запись
                LockMode::WriteQueue => {
                    stat.write_queue.push_back((uuid, unixtime));
                }
            };
        }
        Ok(stat)
    }
}

impl Display for LockInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Блокировка на чтение
        for (uuid, unixtime) in &self.read {
            writeln!(f, "{}={}={}", uuid, unixtime, LockMode::Read)?;
        }

        // Блокировка на запись
        if let Some((uuid, unixtime)) = &self.write {
            writeln!(f, "{}={}={}", uuid, unixtime, LockMode::Write)?;
        }

        // Очередь на запись
        for (uuid, unixtime) in &self.write_queue {
            writeln!(f, "{}={}={}", uuid, unixtime, LockMode::WriteQueue)?;
        }

        Ok(())
    }
}

impl LockInfo {
    pub fn remove<U: AsRef<FsUuid>>(&mut self, uuid: U) {
        let uuid = uuid.as_ref();
        self.read.retain(|(old_uuid, _)| old_uuid != uuid);
        if let Some((old_uuid, _)) = &self.write
            && old_uuid == uuid
        {
            self.write = None;
        }
        self.write_queue.retain(|(old_uuid, _)| old_uuid != uuid);
    }

    pub fn set<U: AsRef<FsUuid>>(
        &mut self,
        uuid: U,
        unixtime: u32,
        mode: LockMode,
    ) -> Result<(), ()> {
        let uuid = uuid.as_ref();

        match mode {
            // Помечаем что есть читатель
            LockMode::Read => {
                // Заблокирован для записи или в очереди на запись
                if self.write.is_some() || !self.write_queue.is_empty() {
                    return Err(());
                }
                // Удаляем старую запись (если есть)
                self.remove(uuid);
                // Добавляем новую запись
                self.read.push((*uuid, unixtime));
            }
            // Добавляем в очередь на запись
            LockMode::WriteQueue => {
                if let Some(index) = self
                    .write_queue
                    .iter_mut()
                    .position(|(in_uuid, _)| in_uuid == uuid)
                {
                    // Если есть в очереди на запись, обновляем время блокировки
                    self.write_queue[index].1 = unixtime;
                } else {
                    // Если нет, добавляем в конец очереди
                    self.write_queue.push_back((*uuid, unixtime));
                }
            }
            // Устанавливаем писатель
            LockMode::Write => {
                if !self.read.is_empty() {
                    return Err(());
                }

                // Очередь должна быть пуста и первый элемент должен совпадать с запрашиваемым uuid
                if self
                    .write_queue
                    .front()
                    .is_some_and(|(first_uuid, _)| first_uuid != uuid)
                {
                    return Err(());
                }

                // Убираем из очереди и устанавливаем его как писатель
                let _ = self.write_queue.pop_front();
                self.write = Some((*uuid, unixtime));
            }
        };

        Ok(())
    }

    /// Парсит строку одной записи.
    ///
    /// @param line Строка для парсинга.
    /// @return Опциональная кортеж `(uuid, unixtime, mode)`, если строка была успешно распаршена.
    fn parse_line(line: &str) -> Result<Option<(FsUuid, u32, LockMode)>, DriverError> {
        let line = line.trim();
        if line.is_empty() {
            return Ok(None);
        }

        let parts: Vec<&str> = line.split('=').collect();
        if parts.len() != 3 {
            return Err(DriverError::ParseLockError {
                reason: format!("Неправильный формат строки: {line}"),
            });
        }

        let uuid: FsUuid = parts[0]
            .parse::<FsUuid>()
            .map_err(|e| DriverError::ParseLockError {
                reason: format!("Неверный формат UUID в строке: {line}. {e}"),
            })?;
        let unixtime = parts[1]
            .parse::<u32>()
            .map_err(|e| DriverError::ParseLockError {
                reason: format!("Неверный формат unixtime в строке: {line}. {e}"),
            })?;
        let mode = LockMode::from_str(parts[2])?;

        Ok(Some((uuid, unixtime, mode)))
    }

    /// Удалить устаревшие блокировки (unixtime + 5 минут < now).
    ///
    /// @param now Текущее unixtime на сервере
    pub(crate) fn remove_stale(&mut self, now: u32) {
        let stale_time = now.saturating_sub(5 * 60);
        self.read.retain(|(_, unixtime)| *unixtime > stale_time);
        self.write_queue
            .retain(|(_, unixtime)| *unixtime > stale_time);

        if let Some((_, unixtime)) = &self.write
            && *unixtime < stale_time
        {
            self.write = None;
        }
    }

    /// Вычислить хеш содержимого блокировки.
    /// Использует [DefaultHasher] для вычисления хеша.
    ///
    /// @return Вычисленный хеш
    pub(crate) fn get_hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
    }
}

#[cfg(test)]
mod tests {
    use crate::{lock::LockInfo, uuid::FsUuid};

    use super::lock_path;
    use std::{path::PathBuf, str::FromStr};
    use tracing_test::traced_test;

    #[test]
    #[traced_test]
    fn test_lock_path() {
        for (path, expected) in [
            ("a/b/c.txt", "a/b/.c.txt.lock"),
            ("a/b/.c.txt", "a/b/.c.txt.lock"),
            ("a/b/txt", "a/b/.txt.lock"),
        ] {
            let lock_path = lock_path(path).unwrap();
            assert_eq!(lock_path, PathBuf::from(expected));
        }
    }

    #[test]
    #[traced_test]
    fn test_serialize() {
        let uuid_default = FsUuid {
            connection_id: 1234,
            copy_id: 1,
        };
        assert_eq!(LockInfo::from_str("").unwrap(), LockInfo::default());

        let lock_content = "1234_1=1780118532=read";
        let lock_stat = LockInfo::from_str(lock_content).unwrap();
        assert_eq!(
            lock_stat,
            LockInfo {
                read: [(uuid_default, 1780118532)].into_iter().collect(),
                write: None,
                write_queue: Default::default(),
            }
        );
        assert_eq!(lock_stat.to_string().trim(), lock_content);

        let lock_content = "1234_1=1780118532=read\n\
        1234_2=1780118532=read\n\
        1234_3=1780118532=read";
        let lock_stat = LockInfo::from_str(lock_content).unwrap();
        assert_eq!(
            lock_stat,
            LockInfo {
                read: [
                    (uuid_default, 1780118532),
                    (
                        FsUuid {
                            connection_id: 1234,
                            copy_id: 2,
                        },
                        1780118532
                    ),
                    (
                        FsUuid {
                            connection_id: 1234,
                            copy_id: 3,
                        },
                        1780118532
                    ),
                ]
                .into_iter()
                .collect(),
                write: None,
                write_queue: Default::default(),
            }
        );

        assert_eq!(lock_stat.to_string().trim(), lock_content);

        // Вторая запись перезапишет первую, так как активен может быть только один писатель.
        let lock_content = "1234_1=1780118532=write\n\
        1234_2=1780118532=write\n";
        let lock_stat = LockInfo::from_str(lock_content).unwrap();
        assert_eq!(
            lock_stat,
            LockInfo {
                read: Vec::new(),
                write: Some((
                    FsUuid {
                        connection_id: 1234,
                        copy_id: 2,
                    },
                    1780118532
                )),
                write_queue: Default::default(),
            }
        );

        let lock_content = "1234_1=1780118532=write_queue\n\
        1234_2=1780118532=write_queue\n\
        1234_3=1780118532=write_queue";
        let lock_stat = LockInfo::from_str(lock_content).unwrap();

        assert_eq!(
            lock_stat,
            LockInfo {
                read: Vec::new(),
                write: None,
                write_queue: [
                    (uuid_default, 1780118532),
                    (
                        FsUuid {
                            connection_id: 1234,
                            copy_id: 2
                        },
                        1780118532
                    ),
                    (
                        FsUuid {
                            connection_id: 1234,
                            copy_id: 3
                        },
                        1780118532
                    ),
                ]
                .into_iter()
                .collect()
            }
        );

        assert_eq!(lock_stat.to_string().trim(), lock_content);
    }
}
