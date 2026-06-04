use fs4me_interface::{Driver, DriverError, DriverParams, Stat, WriteMode};
use std::{
    fmt::Debug,
    fs::{self, File, OpenOptions},
    io::{self, BufWriter, ErrorKind, Seek},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tracing::{debug, instrument, warn};

const DRIVER_VERSION: &str = env!("CARGO_PKG_VERSION");
const DRIVER_NAME: &str = env!("CARGO_PKG_NAME");

#[derive(Debug, Clone)]
pub struct LocalDriver;

impl Driver for LocalDriver {
    /// Возвращает название драйвера.
    fn name(&self) -> &str {
        DRIVER_NAME
    }

    /// Возвращает версию драйвера.
    fn version(&self) -> &str {
        DRIVER_VERSION
    }

    /// Подключается к локальной файловой системе.
    fn connect<P: Into<DriverParams>>(_params: P) -> Result<Self, DriverError> {
        Ok(LocalDriver)
    }

    /// Отключается от локальной файловой системы.
    /// Так как это работа с локальной файловой системой, отключение не требуется.
    fn disconnect(&self) -> Result<(), DriverError> {
        Ok(())
    }

    /// Возвращает текущее время сервера.
    ///
    /// @return Возвращает текущее время в формате Unix timestamp.
    fn time(&self) -> Result<Duration, DriverError> {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| DriverError::ServerTimeError(e.to_string()))
    }

    /// Проверяет, существует ли путь.
    ///
    /// @return Возвращает true, если путь существует.
    fn exists<P>(&self, path: P) -> bool
    where
        P: AsRef<Path>,
    {
        path.as_ref().exists()
    }

    /// Возращает информацию о файле или директории.
    ///
    /// @param path Путь к файлу или директории.
    /// @return Информация о файле или директории.
    fn stat<P>(&self, path: P) -> Result<Stat, DriverError>
    where
        P: AsRef<Path>,
    {
        let path = path.as_ref();
        let metadata = fs::metadata(path).map_err(|err| DriverError::StatError {
            path: path.to_path_buf(),
            reason: err.to_string(),
        })?;
        let modified = metadata
            .modified()
            .map_err(|err| DriverError::LastModifiedError {
                path: path.to_path_buf(),
                reason: err.to_string(),
            })?
            .duration_since(UNIX_EPOCH)
            .map_err(|err| DriverError::ServerTimeError(err.to_string()))?;

        if metadata.is_file() {
            Ok(Stat::File {
                size: metadata.len(),
                modified,
            })
        } else if metadata.is_dir() {
            Ok(Stat::Dir { modified })
        } else {
            Err(DriverError::StatError {
                path: path.to_path_buf(),
                reason: "Неподдерживаемый тип файла".to_string(),
            })
        }
    }

    /// Возвращает итератор по файлам в директории.
    ///
    /// @param path Путь к директории.
    /// @return Итератор по файлам в директории.
    #[instrument(level = "debug", skip(self))]
    fn ls<P>(&self, path: P) -> Result<impl Iterator<Item = PathBuf>, DriverError>
    where
        P: AsRef<Path> + Debug,
    {
        let path = path.as_ref();
        let path: PathBuf =
            path.canonicalize()
                .map_err(|error| DriverError::PathResolutionError {
                    path: path.to_path_buf(),
                    reason: error.to_string(),
                })?;

        if !path.is_dir() {
            return Err(DriverError::NotADirectoryError(path));
        }

        // Возвращаем интератор
        Ok(fs::read_dir(&path)
            .map_err(|err| DriverError::ReadDirError {
                path,
                reason: err.to_string(),
            })?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.is_dir() || path.is_file()))
    }

    /// Перемещает/переименовывает файл/директорию.
    ///
    /// @param from Исходный путь.
    /// @param to Целевой путь.
    /// @return Результат операции.
    #[instrument(level = "debug", skip(self))]
    fn rename<P, Q>(&self, from: P, to: Q) -> Result<(), DriverError>
    where
        P: AsRef<Path> + Debug,
        Q: AsRef<Path> + Debug,
    {
        let from = from.as_ref();
        let to = to.as_ref();
        debug!(?from, ?to, "from->to");
        fs::rename(from, to).map_err(|err| {
            let from = from.to_path_buf();
            match err.kind() {
                ErrorKind::NotFound => DriverError::PathExistsError(from),
                _ => DriverError::MvError {
                    from,
                    to: to.to_path_buf(),
                    reason: err.to_string(),
                },
            }
        })
    }

    /// Создать директорию.
    ///
    /// @param path Путь к директории.
    /// @param recursive Если `true`, то создается директория и все промежуточные директории.
    #[instrument(level = "debug", skip(self))]
    fn mkdir<P>(&self, path: P, recursive: bool) -> Result<(), DriverError>
    where
        P: AsRef<Path> + Debug,
    {
        let path = path.as_ref();
        if self.exists(path) {
            return Err(DriverError::PathExistsError(path.to_path_buf()));
        }

        let result = if recursive {
            fs::create_dir_all(path)
        } else {
            fs::create_dir(path)
        };

        result.map_err(|err| DriverError::MkdirError {
            path: path.to_path_buf(),
            reason: err.to_string(),
        })
    }

    /// Удалить файл/директорию.
    ///
    /// @param path Путь к файлу/директории.
    /// @return `Ok` при успешном удалении, `Err` при ошибке.
    #[instrument(level = "debug", skip(self))]
    fn rm<P>(&self, path: P) -> Result<(), DriverError>
    where
        P: AsRef<Path> + Debug,
    {
        let path = path.as_ref();
        match self.stat(path)? {
            Stat::Dir { .. } => fs::remove_dir_all(path).map_err(|err| DriverError::RmError {
                path: path.to_path_buf(),
                reason: err.to_string(),
            }),
            Stat::File { .. } => fs::remove_file(path).map_err(|err| DriverError::RmError {
                path: path.to_path_buf(),
                reason: err.to_string(),
            }),
        }
    }

    /// Чтение файла.
    ///
    /// @param path Путь к файлу.
    /// @param position Позиция в файле.
    /// @return `Ok` при успешном чтении, `Err` при ошибке.
    #[instrument(level = "debug", skip(self))]
    fn read<P>(&self, path: &P, position: u64) -> Result<Box<dyn std::io::Read>, DriverError>
    where
        P: AsRef<Path> + Debug,
    {
        // Преобразуем входной путь в ссылаемый путь, чтобы можно было использовать его в отчетах об ошибках
        let path = path.as_ref();

        debug!("Открытие файла только для чтения");
        let mut file = OpenOptions::new()
            .write(false)
            .read(true)
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

    /// Возвращает буферизированный писатель для записи в файл по указанному пути.
    ///
    /// @param path Путь к файлу для записи.
    /// @param mode Режим записи (перезапись или добавление).
    /// @return Буферизированный писатель для записи в файл.
    #[instrument(level = "debug", skip(self))]
    fn write<P>(&self, path: &P, mode: WriteMode) -> Result<Box<dyn std::io::Write>, DriverError>
    where
        P: AsRef<Path> + Debug,
    {
        let path = path.as_ref();
        let mut options = OpenOptions::new();
        // Устанавливаем флаг записи; по умолчанию файл создаётся, если его не существует
        options.write(true);

        // Конфигурируем флаги создания в зависимости от выбранного режима
        match mode {
            WriteMode::FailIfExists => options.create_new(true), // Не создавать, если файл есть
            WriteMode::Overwrite => options.create(true).truncate(true), // Создать и обрезать до начала
            WriteMode::Append => options.create(true).append(true), // Создать и добавлять в конец
        };

        debug!("Открытие файла");
        // Открываем файл с указанными опциями. Если ошибка — преобразуем её в DriverError.
        let file = options.open(path).map_err(|err| DriverError::FopenError {
            path: path.to_path_buf(),
            reason: err.to_string(),
        })?;

        debug!("Возвращаем буфер для записи");
        // Оборачиваем дескриптор файла в буферизированный писатель для ускорения I/O
        Ok(Box::new(BufWriter::new(file)))
    }

    /// Копирует файл из `from` в `to`.
    ///
    /// @return Возвращает ошибку, если файл не существует или не является файлом.
    fn copy_file<P, Q>(&self, from: &P, to: &Q) -> Result<(), DriverError>
    where
        P: AsRef<Path> + Debug,
        Q: AsRef<Path> + Debug,
    {
        let from = from.as_ref().to_path_buf();
        let from = from
            .canonicalize()
            .inspect_err(|err| warn!(?from, ?err, "Ошибка при получения полного пути"))
            .map_err(|_| DriverError::PathExistsError(from))?;

        if !from.is_file() {
            debug!(?from, "Должен быть файлом");
            return Err(DriverError::PathNotFileError(from));
        }

        let mut to = to.as_ref().to_path_buf();

        debug!(?from, ?to, "копируем файл from->to=");

        if to.exists() {
            to = to
                .canonicalize()
                .inspect_err(|err| warn!(?to, ?err, "Ошибка при получения полного пути"))
                .map_err(|_| DriverError::PathExistsError(to))?;

            if !to.is_file() {
                debug!(?to, "Если существует то он должен быть файлом");
                return Err(DriverError::PathNotFileError(to));
            }
        }

        if from == to {
            return Ok(());
        }

        fs::copy(&from, &to)
            .map_err(|err| DriverError::CopyError {
                from,
                to,
                reason: err.to_string(),
            })
            .map(|_| ())
    }

    /// Обновляет время последнего изменения файла на текущее.
    ///
    /// @param path - Путь к файлу.
    ///
    /// @return успех или ошибка.
    fn update_file_modified_time_now(&self, path: impl AsRef<Path>) -> Result<(), DriverError> {
        let path = path.as_ref();
        let file = File::open(path).map_err(|err| DriverError::FopenError {
            path: path.to_path_buf(),
            reason: err.to_string(),
        })?;
        file.set_modified(SystemTime::now()).map_err(|err| {
            DriverError::UpdateFileModifiedTimeError {
                path: path.to_path_buf(),
                reason: err.to_string(),
            }
        })?;
        Ok(())
    }
}
