use std::{
    collections::VecDeque,
    fmt::Display,
    path::{Path, PathBuf},
    str::FromStr,
};

use fs4me_interface::{Driver, DriverError};

use crate::{Fs, uuid::FsUuid};

/// Получить путь к файлу блокировки для указанного пути.
///
/// @param path Путь к файлу.
/// @returns Путь к файлу блокировки.
pub(crate) fn lock_path<P: AsRef<Path>>(path: P) -> Result<PathBuf, DriverError> {
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

pub struct Lock<'a, D: Driver> {
    fs: &'a Fs<D>,
    source_path: PathBuf,
}

impl<'a, D: Driver> Lock<'a, D> {
    pub fn new(fs: &'a Fs<D>, source_path: PathBuf) -> Self {
        Self { fs, source_path }
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

/// Информация о блокировке файла.
/// Хранит информацию о читателях, писателях и очереди на запись.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
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

impl ToString for LockInfo {
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
    fn test_lock() {
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
