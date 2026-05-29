use std::{
    collections::HashMap,
    fmt::Display,
    io,
    path::{Path, PathBuf},
    str::FromStr,
};

use fs4me_interface::{Driver, DriverError};

use crate::{Fs, uuid::FsUuid};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockMode {
    Read,
    Write,
}

impl FromStr for LockMode {
    type Err = DriverError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "read" => Ok(LockMode::Read),
            "write" => Ok(LockMode::Write),
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
        }
    }
}

/// Получить путь к файлу блокировки для указанного пути.
///
/// @param path Путь к файлу.
/// @returns Путь к файлу блокировки.
fn lock_path<P: AsRef<Path>>(path: P) -> Result<PathBuf, DriverError> {
    let path = path.as_ref();
    let parent = path
        .parent()
        .ok_or_else(|| DriverError::ParentDirError(path.to_path_buf()))?;
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

/// Читает файл блокировки и возвращает информацию о блокировке.
///
/// @param fs Файловая система.
/// @param path Путь к файлу.
/// @return Результат: информации о блокировке или None, если файл блокировки не существует.
fn read_lock_stat<D: Driver, P: AsRef<Path>>(
    fs: &Fs<D>,
    path: P,
) -> Result<Option<LockStat>, DriverError> {
    let lock_file = lock_path(path)?;
    if !fs.exists(&lock_file) {
        return Ok(None);
    }
    let lock_reader = fs.driver.read(&lock_file, 0)?;
    let lock_content =
        io::read_to_string(lock_reader).map_err(|err| DriverError::ReadSeekError {
            path: lock_file.to_path_buf(),
            reason: err.to_string(),
        })?;
    let mut lock_stat = LockStat::from_str(&lock_content)?;
    let now = fs.time()?;
    lock_stat.remove_stale(now);

    Ok(Some(lock_stat))
}

/// Проверяет, заблокирован ли файл для чтения или записи.
///
/// @param fs Файловая система.
/// @param path Путь к файлу.
/// @param mode Режим блокировки.
/// @returns `true`, если файл заблокирован, `false` — если нет.
pub(crate) fn is_locked<D: Driver, P: AsRef<Path>>(
    fs: &Fs<D>,
    path: P,
    mode: LockMode,
) -> Result<bool, DriverError> {
    match read_lock_stat(fs, path)? {
        Some(stat) => Ok(stat.is_locked(fs, mode)),
        None => Ok(false),
    }
}

/// Информация о блокировке файла.
/// Хранит информацию о читателях, писателях и очереди на запись.
#[derive(Debug, Default, Clone)]
pub struct LockStat {
    /// Карта читателей: uuid -> unixtime блокировки
    read: HashMap<FsUuid, u32>,

    /// Карта писателей: uuid -> unixtime блокировки
    write: Option<(FsUuid, u32)>,

    /// Очередь на запись: список uuid ожидающих блокировки.
    write_queue: Vec<(FsUuid, u32)>,
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
impl FromStr for LockStat {
    type Err = DriverError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut stat = LockStat::default();
        for line in s.lines() {
            let Some((uuid, unixtime, mode)) = LockStat::parse_line(line)? else {
                continue;
            };
            match mode {
                LockMode::Read => {
                    stat.read.insert(uuid, unixtime);
                }
                LockMode::Write => {
                    stat.write = Some((uuid, unixtime));
                }
            };
        }
        Ok(stat)
    }
}

impl LockStat {
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

    /// Формирует строковое представление файла блокировки.
    ///
    /// Каждая строка: uuid=unixtime=mode
    fn to_string(&self) -> String {
        let mut result = String::new();

        // Добавляем читателей
        for (uuid, unixtime) in &self.read {
            result.push_str(&format!("{}={}={}\n", uuid, unixtime, LockMode::Read));
        }

        // Добавляем писателей
        if let Some((uuid, unixtime)) = &self.write {
            result.push_str(&format!("{}={}={}\n", uuid, unixtime, LockMode::Write));
        }

        result
    }

    /// Удалить устаревшие блокировки (unixtime + 5 минут < now).
    ///
    /// @param now Текущее unixtime на сервере
    fn remove_stale(&mut self, now: u32) {
        let stale_time = now.saturating_sub(5 * 60);
        self.read.retain(|_, unixtime| *unixtime > stale_time);

        if let Some((_, unixtime)) = &self.write
            && *unixtime < stale_time
        {
            self.write = None;
        }
    }

    /// Проверить, заблокирован ли файл для чтения или записи.
    ///
    /// @param fs Файловая система, для которой проверяется блокировка
    /// @param mode Режим блокировки (чтение или запись)
    fn is_locked<D: Driver>(&self, fs: &Fs<D>, mode: LockMode) -> bool {
        let client_uuid = &fs.uuid;

        // Проверка на наличие исключительной блокировки на запись у другого клиента.
        if let Some((uuid, _)) = &self.write
            && uuid != client_uuid
        {
            return true;
        }

        match mode {
            LockMode::Read => !self.write_queue.is_empty(), // Нельзя читать, если есть ожидающие записи
            LockMode::Write => self.write.is_some() && !self.read.is_empty(), // Нельзя писать, если есть писатели или читатели
        }
    }
}
