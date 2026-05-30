use fs4me_interface::{Driver, DriverError, DriverParams, Stat, WriteMode};
use std::{
    fs::{self, OpenOptions},
    io::{self, BufWriter, Seek},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

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
    fn server_time(&self) -> Result<u32, DriverError> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| DriverError::ServerTimeError(e.to_string()))?
            .as_secs() as u32;
        Ok(now)
    }

    /// Проверяет, существует ли путь.
    ///
    /// @return Возвращает true, если путь существует.
    fn exists<P: AsRef<Path>>(&self, path: P) -> bool {
        path.as_ref().exists()
    }

    /// Возращает информацию о файле или директории.
    ///
    /// @param path Путь к файлу или директории.
    /// @return Информация о файле или директории.
    fn stat<P: AsRef<Path>>(&self, path: P) -> Result<Stat, DriverError> {
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
            .map_err(|err| DriverError::ServerTimeError(err.to_string()))?
            .as_secs() as u32;
        if metadata.is_file() {
            Ok(Stat::File {
                size: metadata.len(),
                modified,
            })
        } else {
            Ok(Stat::Dir { modified })
        }
    }

    /// Возвращает итератор по файлам в директории.
    ///
    /// @param path Путь к директории.
    /// @return Итератор по файлам в директории.
    fn ls<P: AsRef<Path>>(&self, path: P) -> Result<impl Iterator<Item = PathBuf>, DriverError> {
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
            .map(|entry| entry.path()))
    }

    /// Перемещает/переименовывает файл/директорию.
    ///
    /// @param from Исходный путь.
    /// @param to Целевой путь.
    /// @return Результат операции.
    fn mv<P: AsRef<Path>, Q: AsRef<Path>>(&self, from: P, to: Q) -> Result<(), DriverError> {
        let from = from.as_ref();
        let to = to.as_ref();
        fs::rename(from, to).map_err(|err| DriverError::MvError {
            old_path: from.to_path_buf(),
            new_path: to.to_path_buf(),
            reason: err.to_string(),
        })
    }

    /// Создать директорию.
    ///
    /// @param path Путь к директории.
    /// @param recursive Если `true`, то создается директория и все промежуточные директории.
    fn mkdir<P: AsRef<Path>>(&self, path: P, recursive: bool) -> Result<(), DriverError> {
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
    fn rm<P: AsRef<Path>>(&self, path: P) -> Result<(), DriverError> {
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
    fn read<P: AsRef<Path>>(
        &self,
        path: &P,
        position: u64,
    ) -> Result<Box<dyn std::io::Read>, DriverError> {
        // Преобразуем входной путь в ссылаемый путь, чтобы можно было использовать его в отчетах об ошибках
        let path = path.as_ref();
        // Открываем файл только для чтения
        let mut file = OpenOptions::new()
            .write(false)
            .read(true)
            .open(path)
            .map_err(|err| DriverError::FopenError {
                // Включаем полный путь в ошибку, чтобы было понятно, с каким файлом возникла проблема
                path: path.to_path_buf(),
                reason: err.to_string(),
            })?;
        // Переходим к указанной позиции в файле
        file.seek(std::io::SeekFrom::Start(position))
            .map_err(|err| DriverError::ReadSeekError {
                path: path.to_path_buf(),
                reason: err.to_string(),
            })?;
        // Оборачиваем файловый дескриптор в буферизированный читатель.
        // Буферизация ускоряет операции чтения за счет минимизации системных вызовов I/O.
        let buf_reader = io::BufReader::new(file);
        Ok(Box::new(buf_reader))
    }

    /// Возвращает буферизированный писатель для записи в файл по указанному пути.
    ///
    /// @param path Путь к файлу для записи.
    /// @param mode Режим записи (перезапись или добавление).
    /// @return Буферизированный писатель для записи в файл.
    fn write<P: AsRef<Path>>(
        &self,
        path: &P,
        mode: WriteMode,
    ) -> Result<Box<dyn std::io::Write>, DriverError> {
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

        // Открываем файл с указанными опциями. Если ошибка — преобразуем её в DriverError.
        let file = options.open(path).map_err(|err| DriverError::FopenError {
            path: path.to_path_buf(),
            reason: err.to_string(),
        })?;

        // Оборачиваем дескриптор файла в буферизированный писатель для ускорения I/O
        Ok(Box::new(BufWriter::new(file)))
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;
    use tracing::info;
    use tracing_test::traced_test;

    use fs4me_interface::{Driver, DriverParams, WriteMode};

    use crate::LocalDriver;

    #[test]
    #[traced_test]
    fn test_driver_info() {
        let driver = LocalDriver::connect(DriverParams::default()).unwrap();
        let name = driver.name();
        info!("Name: {name}");
        let version = driver.version();
        info!("Version: {version}");
        assert!(!name.is_empty());
        assert!(!version.is_empty());
    }

    #[test]
    #[traced_test]
    fn test_time() {
        let driver = LocalDriver::connect(DriverParams::default()).unwrap();
        let server_time = driver.server_time().unwrap();
        info!("Server time: {server_time}");
        let local_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        info!("Local time: {local_time}");
    }

    #[test]
    #[traced_test]
    fn test_ls() {
        let tmp_dir = tempdir().unwrap();
        let root_path = tmp_dir.path();
        let dir_0 = root_path.join("0");

        info!("Временная директория: {root_path:?}");

        let driver = LocalDriver::connect(DriverParams::default()).unwrap();

        let mut iter = driver.ls(root_path).unwrap();
        assert!(iter.next().is_none(), "Директория должна быть пустой");
        assert!(!driver.exists(&dir_0), "Директория не должна существовать");

        // Создание директорий
        for dir_name in 0..10 {
            let dir_path = root_path.join(dir_name.to_string());
            driver.mkdir(dir_path, false).unwrap();
        }

        let files = driver.ls(root_path).unwrap().collect::<Vec<_>>();
        assert_eq!(files.len(), 10, "Должно быть 10 директорий");

        assert!(driver.exists(&dir_0), "Директория должна существовать");

        driver.rm(&dir_0).unwrap();
        assert!(!driver.exists(&dir_0), "Директория должна быть удалена");
    }

    /// Тестирование переименования/перемещения директорий с вложенными директориями.
    #[test]
    #[traced_test]
    fn test_rename() {
        let tmp_dir = tempdir().unwrap();
        info!("Временная директория: {:?}", tmp_dir.path());

        let driver = LocalDriver::connect(DriverParams::default()).unwrap();

        let root = tmp_dir.path();
        let a = root.join("a");
        let a1 = a.join("a1");
        let b = root.join("b");

        driver.mkdir(&a1, true).unwrap();
        assert!(driver.exists(&a), "Директория {a:?} должна существовать");
        assert!(
            !driver.exists(&b),
            "Директория {b:?} не должна существовать"
        );
        driver.mv(&a, &b).unwrap();
        assert!(
            !driver.exists(&a),
            "Директория {a:?} не должна существовать после переименования"
        );
        assert!(
            driver.exists(&b),
            "Директория {b:?} должна существовать после переименования"
        );
        assert!(
            driver.exists(b.join("a1")),
            "Директория `b/a1` должна существовать"
        );
    }

    /// Тестирование работы LocalDriver с директориями.
    /// Проверяет создание, удаление, перечисление и проверку существования директорий.
    #[test]
    #[traced_test]
    fn test_work_with_directory() {
        let tmp_dir = tempdir().unwrap();
        info!("Временная директория: {:?}", tmp_dir.path());

        let driver = LocalDriver::connect(DriverParams::default()).unwrap();

        let root = tmp_dir.path();
        let a = root.join("a");
        let a1 = a.join("a1");
        let a2 = a1.join("a2");

        // Проверка начального состояния
        assert!(
            driver.ls(root).unwrap().next().is_none(),
            "Директория должна быть пустой"
        );
        assert!(
            !driver.exists(&a1),
            "Директория ./a/a1 не должна существовать"
        );
        assert!(
            driver.mkdir(&a1, false).is_err(),
            "Нельзя создать ./a/a1, так как ./a не существует"
        );

        // Создание рекурсивной структуры
        driver.mkdir(&a2, true).unwrap();
        assert!(
            driver.exists(&a2),
            "Директория ./a/a1/a2 должна существовать"
        );

        // Создание простых директорий в корне
        for dir_name in ["b", "c", "d"] {
            let path = root.join(dir_name);
            driver.mkdir(&path, false).unwrap();
        }

        assert_eq!(
            driver.ls(root).unwrap().count(),
            4,
            "В корне должно быть 4 директории: a, b, c, d"
        );

        // Перемещение в корзину
        driver
            .rm(&a)
            .expect("Должно быть успешно удалено целое дерево ./a");
        assert!(!driver.exists(&a), "Директория ./a должна быть удалена");

        assert_eq!(
            driver.ls(&root).unwrap().count(),
            3,
            "В корне должно быть 4 директории: b, c, d, .trash"
        );
    }

    #[test]
    #[traced_test]
    fn test_rw() {
        let tmp_dir = tempdir().unwrap();
        info!("Временная директория: {:?}", tmp_dir.path());

        let driver = LocalDriver::connect(DriverParams::default()).unwrap();

        let root = tmp_dir.path();
        let file_path = root.join("demo.txt");
        info!("Создание файла {file_path:?}. Только если его нет");

        // Открытие для записи только если файл не существует
        {
            let mut fopen = driver.write(&file_path, WriteMode::FailIfExists).unwrap();
            assert!(
                driver.exists(&file_path),
                "Файл {file_path:?} должен существовать после его открытия"
            );
            writeln!(&mut fopen, "a").unwrap();
            drop(fopen);
        }
        // Тестирование чтения
        {
            let mut fopen = driver.read(&file_path, 0).unwrap();
            let mut buf = String::new();
            fopen.read_to_string(&mut buf).unwrap();
            assert_eq!(buf, "a\n");
        }

        // попытка записи в существующий файл с флагом FailIfExists запрещающий запись в существующий файл
        {
            assert!(
                driver.write(&file_path, WriteMode::FailIfExists).is_err(),
                "Должно быть ошибка при записи в существующий файл"
            );
        }

        // дозапись
        {
            let mut fopen = driver.write(&file_path, WriteMode::Append).unwrap();
            writeln!(&mut fopen, "b").unwrap();
            drop(fopen);
        }
        // тестирование чтения с указанием позиции
        {
            let mut fopen = driver.read(&file_path, 2).unwrap();
            let mut buf = String::new();
            fopen.read_to_string(&mut buf).unwrap();
            assert_eq!(buf, "b\n");
        }

        // тестирование перезаписи
        {
            let mut fopen = driver.write(&file_path, WriteMode::Overwrite).unwrap();
            write!(&mut fopen, "c").unwrap();
            drop(fopen);
        }
        // тестирование чтения после перезаписи
        {
            let mut fopen = driver.read(&file_path, 0).unwrap();
            let mut buf = String::new();
            fopen.read_to_string(&mut buf).unwrap();
            assert_eq!(buf, "c");
        }
    }
}
