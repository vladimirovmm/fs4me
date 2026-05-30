use std::{
    collections::VecDeque,
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

/// Проверяет, можно ли выполнить действие над файлом/директорией.
///
/// @param fs Файловая система.
/// @param path Путь к файлу.
/// @param mode Режим блокировки.
/// @returns `true`, если действие можно выполнить, `false` — если нет.
pub(crate) fn is_operation_allowed<D: Driver, P: AsRef<Path>>(
    fs: &Fs<D>,
    path: P,
    mode: LockMode,
) -> Result<bool, DriverError> {
    match read_lock_stat(fs, path)? {
        Some(stat) => Ok(stat.is_operation_allowed(fs, mode)),
        None => Ok(false),
    }
}

/// Информация о блокировке файла.
/// Хранит информацию о читателях, писателях и очереди на запись.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LockStat {
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
impl FromStr for LockStat {
    type Err = DriverError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut stat = LockStat::default();
        for line in s.lines() {
            let Some((uuid, unixtime, mode)) = LockStat::parse_line(line)? else {
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

impl ToString for LockStat {
    fn to_string(&self) -> String {
        let mut result = String::new();

        // Блокировка на чтение
        for (uuid, unixtime) in &self.read {
            result.push_str(&format!("{}={}={}\n", uuid, unixtime, LockMode::Read));
        }

        // Блокировка на запись
        if let Some((uuid, unixtime)) = &self.write {
            result.push_str(&format!("{}={}={}\n", uuid, unixtime, LockMode::Write));
        }

        // Очередь на запись
        for (uuid, unixtime) in &self.write_queue {
            result.push_str(&format!("{}={}={}\n", uuid, unixtime, LockMode::WriteQueue));
        }

        result
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

    /// Удалить устаревшие блокировки (unixtime + 5 минут < now).
    ///
    /// @param now Текущее unixtime на сервере
    fn remove_stale(&mut self, now: u32) {
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

    /// Проверить, можно ли выполнить операцию над файлом/директорией
    ///
    /// @param fs Файловая система, для которой проверяется блокировка
    /// @param mode Режим блокировки (чтение или запись)
    /// @returns `true`, если операцию можно выполнить, `false` — если нет.
    fn is_operation_allowed<U: AsRef<FsUuid>>(&self, uuid: U, mode: LockMode) -> bool {
        let uuid = uuid.as_ref();
        match (mode, self.write.is_some()) {
            (LockMode::WriteQueue, _) => true, // Очередь на запись всегда открыта
            (_, true) => false, // Если существует активная запись, любые другие операции запрещены
            (LockMode::Read, _) => self.write_queue.is_empty(), // Чтение разрешено только если очередь на запись пуста
            // Запись разрешена только если нет читателей и текущий клиент стоит в начале очереди на запись
            (LockMode::Write, _) => {
                self.read.is_empty() && self.write_queue.front().is_some_and(|(u, _)| u == uuid)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        lock::{LockMode, LockStat},
        uuid::FsUuid,
    };

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
    fn test_lock() {
        let uuid_default = FsUuid {
            connection_id: 1234,
            copy_id: 1,
        };
        assert_eq!(LockStat::from_str("").unwrap(), LockStat::default());

        let lock_content = "1234_1=1780118532=read";
        let lock_stat = LockStat::from_str(lock_content).unwrap();
        assert_eq!(
            lock_stat,
            LockStat {
                read: [(uuid_default, 1780118532)].into_iter().collect(),
                write: None,
                write_queue: Default::default(),
            }
        );
        assert_eq!(lock_stat.to_string().trim(), lock_content);

        assert!(
            lock_stat.is_operation_allowed(uuid_default, LockMode::Read),
            "Файл можно читать если другие его тоже читают"
        );
        assert!(
            !lock_stat.is_operation_allowed(uuid_default, LockMode::Write),
            "Файл читается, нельзя начать запись"
        );
        assert!(
            lock_stat.is_operation_allowed(uuid_default, LockMode::WriteQueue),
            "Всегда можно встать на очередь в запись"
        );

        let lock_content = "1234_1=1780118532=read\n\
        1234_2=1780118532=read\n\
        1234_3=1780118532=read";
        let lock_stat = LockStat::from_str(lock_content).unwrap();
        assert_eq!(
            lock_stat,
            LockStat {
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
        let lock_stat = LockStat::from_str(lock_content).unwrap();
        assert_eq!(
            lock_stat,
            LockStat {
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

        assert!(
            !lock_stat.is_operation_allowed(uuid_default, LockMode::Write),
            "Уже есть активный писатель"
        );
        assert!(
            !lock_stat.is_operation_allowed(uuid_default, LockMode::Read),
            "Во время записи нельзя читать"
        );
        assert!(
            lock_stat.is_operation_allowed(uuid_default, LockMode::WriteQueue),
            "Всегда можно встать в очередь"
        );

        let lock_content = "1234_1=1780118532=write_queue\n\
        1234_2=1780118532=write_queue\n\
        1234_3=1780118532=write_queue";
        let lock_stat = LockStat::from_str(lock_content).unwrap();

        assert_eq!(
            lock_stat,
            LockStat {
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
        assert!(
            !lock_stat.is_operation_allowed(uuid_default, LockMode::Read),
            "Нельзя начать чтение, так как есть очередь на запись"
        );
        assert!(
            lock_stat.is_operation_allowed(uuid_default, LockMode::Write),
            "Можно начать запись, так как он первый в очереди"
        );
        assert!(
            !lock_stat.is_operation_allowed(uuid_default, LockMode::Write),
            "Нельзя начать запись, так как есть очередь на запись"
        );
        assert!(
            lock_stat.is_operation_allowed(uuid_default, LockMode::WriteQueue),
            "Всегда можно встать в очередь"
        );
    }
}
