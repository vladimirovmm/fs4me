use base64::{Engine, prelude::BASE64_STANDARD};
use fs4me_interface::{Driver, DriverError, DriverParams, Stat, WriteMode};
use fs4me_macro::DriverFFI;
use ssh2::{OpenFlags, OpenType, Session, Sftp};
use std::{
    fmt::{Debug, Display},
    io::{self, BufWriter, Read, Seek},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime},
};
use tracing::debug;

const DRIVER_VERSION: &str = env!("CARGO_PKG_VERSION");
const DRIVER_NAME: &str = env!("CARGO_PKG_NAME");

/// Драйвер для работы с SFTP.
#[derive(Clone, DriverFFI)]
pub struct SftpDriver {
    sftp: Arc<Sftp>,
    // Временной сдвиг (Локальное время - серверное время)
    time_offset: i64,
}

impl SftpDriver {
    fn to_local_time(&self, server_time: Duration) -> Duration {
        to_local_time(server_time, self.time_offset)
    }

    fn to_server_time(&self, local_time: Duration) -> Duration {
        to_server_time(local_time, self.time_offset)
    }
}

impl Driver for SftpDriver {
    /// Возвращает название драйвера.
    fn name(&self) -> &str {
        DRIVER_NAME
    }

    /// Возвращает версию драйвера.
    fn version(&self) -> &str {
        DRIVER_VERSION
    }

    /// Подключается к SFTP серверу.
    fn connect<P: Into<DriverParams>>(params: P) -> Result<Self, DriverError> {
        let params = params.into();

        // Куда подключаемся host:port
        let host: &str = params
            .get("host")
            .ok_or_else(|| DriverError::ConnectError {
                reason: "Не передан параметр host".to_string(),
            })?
            .as_str();
        let port: u16 = params
            .get("port")
            .map(|p| p.parse::<u16>())
            .transpose()
            .map_err(|err| DriverError::ConnectError {
                reason: format!("Неверный формат порта. {err}"),
            })?
            .unwrap_or(22);
        // От чьего имени подключаемся
        let username = params
            .get("username")
            .ok_or_else(|| DriverError::ConnectError {
                reason: "Не передан параметр username".to_string(),
            })?;
        // Пароль или приватный ключ для аутентификации
        let password = params.get("password");
        let private_key = params
            .get("private_key")
            .map(|k| {
                fn to_error<S: Display>(err: S) -> DriverError {
                    DriverError::ConnectError {
                        reason: format!(
                            "Неверный формат private_key. Ожидается в base64(string). {err}"
                        ),
                    }
                }
                let key = BASE64_STANDARD.decode(k).map_err(to_error)?;
                String::from_utf8(key).map_err(to_error)
            })
            .transpose()?;
        let passphrase: Option<&str> = params.get("passphrase").map(|p| p.as_str());

        // timeout соединения
        let timeout = params
            .get("timeout")
            .map(|t| t.parse::<u32>())
            .transpose()
            .map_err(|err| DriverError::ConnectError {
                reason: format!("Неверный формат timeout. {err}"),
            })?;
        // KeepAlive
        let keepalive = params
            .get("keepalive")
            .map(|k| k.parse::<u32>())
            .transpose()
            .map_err(|err| DriverError::ConnectError {
                reason: format!("Неверный формат keepalive. {err}"),
            })?;

        debug!(%host, %port, %username, "Подключение к SFTP серверу");

        // Создаем TCP соединение
        let stream =
            std::net::TcpStream::connect((host, port)).map_err(|err| DriverError::StatError {
                path: PathBuf::from(host),
                reason: format!("Ошибка подключения к серверу: {}", err),
            })?;

        // Создаем SSH сессию
        let mut session = Session::new().map_err(|err| DriverError::StatError {
            path: PathBuf::from(host),
            reason: format!("Ошибка создания SSH сессии: {}", err),
        })?;

        // Устанавливаем TCP поток
        session.set_tcp_stream(stream);

        // Устанавливаем таймаут соединения (30 секунд)
        if let Some(timeout) = timeout {
            session.set_timeout(timeout);
        }
        // Устанавливаем keepalive
        if let Some(keepalive) = keepalive {
            session.set_keepalive(true, keepalive);
        }

        // Рукопожатие
        session.handshake().map_err(|err| DriverError::StatError {
            path: PathBuf::from(host),
            reason: format!("Ошибка рукопожатия: {}", err),
        })?;

        // Аутентификация
        if let Some(private_key) = private_key {
            debug!("Аутентификация с приватным ключом");

            session
                .userauth_pubkey_memory(username, None, &private_key, passphrase)
                .map_err(|err| DriverError::ConnectError {
                    reason: format!("Ошибка аутентификации по ключу: {err}"),
                })?;
        } else if let Some(password) = password {
            debug!("Аутентификация с паролем");
            session
                .userauth_password(username, password)
                .map_err(|err| DriverError::ConnectError {
                    reason: format!("Ошибка аутентификации по паролю: {err}"),
                })?;
        } else {
            return Err(DriverError::ConnectError {
                reason: "Не передан параметр password или private_key. Укажите пароль или приватный ключ.".to_string(),
            });
        };

        if !session.authenticated() {
            return Err(DriverError::ConnectError {
                reason: "Аутентификация не удалась. Проверьте параметры подключения.".to_string(),
            });
        }

        debug!("Аутентификация успешна");

        debug!("Получение времени сервера");
        let time_offset = {
            let mut ssh_channel =
                session
                    .channel_session()
                    .map_err(|err| DriverError::ConnectError {
                        reason: format!("Ошибка при создании канала сессии. {err}"),
                    })?;
            ssh_channel.exec("date -u +%s").map_err(|err| {
                DriverError::ServerTimeError(format!("Ошибка при выполнении команды date. {err}"))
            })?;

            let mut server_unix_time = String::new();
            ssh_channel
                .read_to_string(&mut server_unix_time)
                .map_err(|err| DriverError::DriverError {
                    reason: format!("Ошибка при чтении ответа: {err}"),
                })?;
            let server_unix_time =
                server_unix_time
                    .trim()
                    .parse::<i64>()
                    .map_err(|err| DriverError::DriverError {
                        reason: format!("Ошибка при парсинге времени: {err}"),
                    })?;
            let time_offset = now().as_secs() as i64 - server_unix_time;
            debug!("Разница во времени: {time_offset:?}");
            ssh_channel
                .wait_close()
                .map_err(|err| DriverError::DriverError {
                    reason: format!("Ошибка при закрытии сессии: {err}"),
                })?;
            time_offset
        };
        let sftp = session.sftp().map_err(|err| DriverError::ConnectError {
            reason: format!("Ошибка создания SFTP сессии: {err}"),
        })?;

        Ok(Self {
            sftp: Arc::new(sftp),
            time_offset,
        })
    }

    /// Отключается от SFTP сервера.
    fn disconnect(&self) -> Result<(), DriverError> {
        // Дроп сессий происходит автоматически при падении refcount
        // Но можно добавить явное логирование
        debug!("Отключение от SFTP сервера");
        Ok(())
    }

    /// Возвращает текущее время сервера.
    fn server_time(&self) -> Result<Duration, DriverError> {
        Ok(self.to_server_time(now()))
    }

    /// Проверяет, существует ли путь.
    fn exists<P>(&self, path: P) -> bool
    where
        P: AsRef<Path>,
    {
        self.stat(path).is_ok()
    }

    /// Возвращает информацию о файле или директории.
    fn stat<P>(&self, path: P) -> Result<Stat, DriverError>
    where
        P: AsRef<Path>,
    {
        let path = path.as_ref();
        self.sftp
            .stat(path)
            .map(|stat| {
                let modified = stat
                    .mtime
                    .map(Duration::from_secs)
                    .map(|time| self.to_local_time(time))
                    .unwrap_or_default();

                if stat.is_dir() {
                    Stat::Dir { modified }
                } else {
                    Stat::File {
                        size: stat.size.unwrap_or_default(),
                        modified,
                    }
                }
            })
            .map_err(|err| DriverError::StatError {
                path: path.to_path_buf(),
                reason: err.to_string(),
            })
    }

    /// Возвращает итератор по файлам в директории.
    fn ls<P>(&self, path: P) -> Result<impl Iterator<Item = PathBuf>, DriverError>
    where
        P: AsRef<Path> + Debug,
    {
        let path = path.as_ref();
        self.sftp
            .readdir(path)
            .map_err(|err| DriverError::ReadDirError {
                path: path.to_path_buf(),
                reason: err.to_string(),
            })
            .map(|paths| Box::new(paths.into_iter().map(|(path, _)| path)))
    }

    /// Перемещает/переименовывает файл/директорию.
    fn rename<P, Q>(&self, from: P, to: Q) -> Result<(), DriverError>
    where
        P: AsRef<Path> + Debug,
        Q: AsRef<Path> + Debug,
    {
        let from = from.as_ref();
        let to = to.as_ref();
        self.sftp
            .rename(from, to, None)
            .map_err(|err| DriverError::MvError {
                from: from.to_path_buf(),
                to: to.to_path_buf(),
                reason: err.to_string(),
            })
    }

    /// Создает директорию.
    fn mkdir<P>(&self, path: P, recursive: bool) -> Result<(), DriverError>
    where
        P: AsRef<Path> + Debug,
    {
        let path = path.as_ref();

        if !recursive {
            return self
                .sftp
                .mkdir(path, 0o755)
                .map_err(|err| DriverError::MkdirError {
                    path: path.to_path_buf(),
                    reason: err.to_string(),
                });
        }

        let mut current_dir = PathBuf::from("/");
        for component in path.components() {
            current_dir.push(component);

            debug!(?current_dir, "mkdir: проверяем существование директории");
            if self.exists(&current_dir) {
                continue;
            }
            self.mkdir(&current_dir, false)?;
        }
        Ok(())
    }

    /// Удаляет файл/директорию.
    fn rm<P>(&self, path: P) -> Result<(), DriverError>
    where
        P: AsRef<Path> + Debug,
    {
        let path = path.as_ref();
        match self.stat(path)? {
            Stat::Dir { .. } => {
                for entry in self.ls(path)? {
                    self.rm(entry)?;
                }
                //
                self.sftp.rmdir(path).map_err(|err| DriverError::RmError {
                    path: path.to_path_buf(),
                    reason: err.to_string(),
                })
            }
            Stat::File { .. } => self.sftp.unlink(path).map_err(|err| DriverError::RmError {
                path: path.to_path_buf(),
                reason: err.to_string(),
            }),
        }
    }

    /// Чтение файла.
    fn read<P>(&self, path: &P, position: u64) -> Result<Box<dyn std::io::Read>, DriverError>
    where
        P: AsRef<Path> + Debug,
    {
        let path = path.as_ref();
        let mut file = self
            .sftp
            .open(path)
            .map_err(|err| DriverError::FopenError {
                // Включаем полный путь в ошибку, чтобы было понятно, с каким файлом возникла проблема
                path: path.to_path_buf(),
                reason: err.to_string(),
            })?;

        debug!("Переходим к указанной позиции в файле");
        file.seek(std::io::SeekFrom::Start(position))
            .map_err(|err| DriverError::ReadSeekError {
                path: path.to_path_buf(),
                reason: err.to_string(),
            })?;
        // Оборачиваем файловый дескриптор в буферизированный читатель.
        // Буферизация ускоряет операции чтения за счет минимизации системных вызовов I/O.
        let buf_reader = io::BufReader::new(file);

        debug!("Возвращаем буфер для чтения");
        Ok(Box::new(buf_reader))
    }

    /// Возвращает буферизированный писатель для записи в файл.
    fn write<P>(&self, path: &P, mode: WriteMode) -> Result<Box<dyn std::io::Write>, DriverError>
    where
        P: AsRef<Path> + Debug,
    {
        let path = path.as_ref();

        let flags = match mode {
            WriteMode::FailIfExist => OpenFlags::EXCLUSIVE,
            WriteMode::Overwrite => OpenFlags::CREATE | OpenFlags::TRUNCATE,
            WriteMode::Append => OpenFlags::CREATE | OpenFlags::APPEND,
        };

        debug!("Открытие файла");
        let file = self
            .sftp
            .open_mode(path, flags, 0o755, OpenType::File)
            .map_err(|err| DriverError::FopenError {
                path: path.to_path_buf(),
                reason: err.to_string(),
            })?;

        debug!("Возвращаем буфер для записи");
        // Оборачиваем дескриптор файла в буферизированный писатель для ускорения I/O
        Ok(Box::new(BufWriter::new(file)))
    }

    /// Копирует файл из `from` в `to`.
    fn copy_file<P, Q>(&self, from: &P, to: &Q) -> Result<(), DriverError>
    where
        P: AsRef<Path> + Debug,
        Q: AsRef<Path> + Debug,
    {
        let from = from.as_ref().to_path_buf();
        let to = to.as_ref().to_path_buf();

        let mut reader = self.read(&from, 0)?;
        let mut writer = self.write(&to, WriteMode::Overwrite)?;

        debug!("Копирование файла");
        // Читаем блоками по 8 КБ
        let mut buffer = [0u8; 8192];
        loop {
            let bytes_read = reader
                .read(&mut buffer)
                .map_err(|e| DriverError::CopyError {
                    from: from.to_path_buf(),
                    to: to.to_path_buf(),
                    reason: format!("Ошибка при чтении из буфера: {e}"),
                })?;
            if bytes_read == 0 {
                break;
            }
            writer
                .write_all(&buffer[..bytes_read])
                .map_err(|e| DriverError::CopyError {
                    from: from.to_path_buf(),
                    to: to.to_path_buf(),
                    reason: format!("Ошибка при записи в буфер: {e}"),
                })?;
        }

        Ok(())
    }

    /// Обновляет время последнего изменения файла на текущее.
    fn update_file_modified_time_now(&self, path: impl AsRef<Path>) -> Result<(), DriverError> {
        let path = path.as_ref();
        let mut stat = self.sftp.stat(path).map_err(|e| DriverError::StatError {
            path: path.to_path_buf(),
            reason: format!("Ошибка при получении статистики: {e}"),
        })?;
        stat.mtime = Some(self.server_time()?.as_secs());
        self.sftp
            .setstat(path, stat)
            .map_err(|e| DriverError::LastModifiedError {
                path: path.to_path_buf(),
                reason: format!("Ошибка при обновлении времени последнего изменения: {e}"),
            })
    }
}

fn now() -> Duration {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
}

fn to_local_time(server_time: Duration, time_offset: i64) -> Duration {
    if time_offset == 0 {
        server_time
    } else if time_offset < 0 {
        server_time + Duration::from_secs(time_offset.unsigned_abs())
    } else {
        server_time - Duration::from_secs(time_offset as u64)
    }
}

fn to_server_time(local_time: Duration, time_offset: i64) -> Duration {
    if time_offset == 0 {
        local_time
    } else if time_offset < 0 {
        local_time - Duration::from_secs(time_offset.unsigned_abs())
    } else {
        local_time + Duration::from_secs(time_offset as u64)
    }
}

#[test]
fn test_convert_time() {
    let local_time = now();
    let server_time = local_time + Duration::from_secs(3 * 60);

    assert_eq!(to_server_time(server_time, 0), server_time);
    assert_eq!(to_server_time(local_time, 3 * 60), server_time);
    assert_eq!(to_local_time(server_time, 0), server_time);
    assert_eq!(to_local_time(server_time, 3 * 60), local_time);
}
