use fs4me_interface::DriverError;
use std::{
    collections::VecDeque,
    fmt::{Debug, Display},
    hash::{DefaultHasher, Hash, Hasher},
    str::FromStr,
};

use crate::{lock::LockMode, uuid::FsUuid};

/// Структура, содержащая информацию о блокировке файла и её статусе.
#[derive(Debug, Default)]
pub(crate) struct LockInfoRead {
    /// Информация о блокировке файла.
    pub lock_info: LockInfo,
    /// Время последнего изменения файла.
    pub modified_time: Option<u32>,
    /// Хеш файла.
    pub hash: Option<u64>,
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

/// Преобразует структуру LockInfo в строковое представление для хранения в файле блокировки (серализация).
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

    /// Проверить, что блокировка пуста.
    /// Нужно для проверки можно ли удалить файл блокировки
    pub(crate) fn is_empty(&self) -> bool {
        self.read.is_empty() && self.write_queue.is_empty() && self.write.is_none()
    }
}

#[cfg(test)]
mod tests {
    use crate::{lock::LockInfo, uuid::FsUuid};

    use std::str::FromStr;
    use tracing_test::traced_test;

    #[test]
    #[traced_test]
    fn test_lock_info() {
        let uuid_default = FsUuid {
            connection_id: 1234,
            copy_id: 1,
        };
        assert_eq!(LockInfo::from_str("").unwrap(), LockInfo::default());

        let lock_content = "\n";
        let lock_stat = LockInfo::from_str(lock_content).unwrap();
        assert_eq!(lock_stat, LockInfo::default());
        assert_eq!(lock_stat.to_string().trim(), lock_content.trim());
        assert!(lock_stat.is_empty(), "Блокировка пуста");

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
        assert!(!lock_stat.is_empty(), "Блокировка не пуста");

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
