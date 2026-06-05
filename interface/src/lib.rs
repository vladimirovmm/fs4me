use std::{
    fmt::Debug,
    io,
    path::{Path, PathBuf},
    time::Duration,
};
use tracing::debug;

mod errors;
mod open_params;
pub use crate::{errors::DriverError, open_params::DriverParams};

/// Режим записи файла.
#[derive(Debug, PartialEq, Eq)]
pub enum WriteMode {
    FailIfExists, // Ошибка, если файл уже существует
    Overwrite,    // Перезаписать файл, если он существует
    Append,       // Добавить данные в конец файла
}

/// Информация о файле/директории.
#[derive(Debug)]
pub enum Stat {
    File {
        /// Размер файла.
        size: u64,
        /// Дата последнего изменения файла. Unix timestamp (UTC)
        modified: Duration,
    },
    Dir {
        /// Дата последнего изменения директории. Unix timestamp (UTC)
        modified: Duration,
    },
}

impl Stat {
    pub fn modified(&self) -> Duration {
        match self {
            Stat::File { modified, .. } => *modified,
            Stat::Dir { modified } => *modified,
        }
    }
}

/// Обеспечивает небезопасный доступ к файловому хранилищу. Т.е. без использования блокировок и управления одновременным доступе.
pub trait Driver: Sized + Clone + Send + Sync + 'static {
    /// Возвращает название драйвера.
    ///
    /// @return &str - название драйвера
    fn name(&self) -> &str;

    /// Возвращает версию драйвера.
    ///
    /// @return &str - версия драйвера
    fn version(&self) -> &str;

    /// Возвращает информацию о драйвере в формате "name + version".
    ///
    /// @return String - информация о драйвере
    fn info(&self) -> String {
        format!("{} v{}", self.name(), self.version())
    }

    /// Подключение к файловому хранилищу с указанными параметрами.
    fn connect<P: Into<DriverParams>>(params: P) -> Result<Self, DriverError>;

    /// Отключение от файлового хранилища.
    ///
    /// При реализации обязательно вызывайте его для drop
    fn disconnect(&self) -> Result<(), DriverError>;

    /// Возвращает время сервера в формате Unix timestamp.
    ///
    /// @return Result<u32> - Результат: Unix timestamp в секундах
    fn time(&self) -> Result<Duration, DriverError>;

    /// Проверяет существование файла или директории.
    ///
    /// @param path - Путь к файлу или директории.
    /// @return bool - Результат: true, если файл или директория существует, false - если нет.
    fn exists<P: AsRef<Path>>(&self, path: P) -> bool;

    /// Возвращает интератор путей файлов и директорий в указанной директории.
    ///
    /// @param path - Путь к директории.
    /// @return Result<impl Iterator<Item = PathBuf>> - Результат: интератор путей
    ///
    /// Пример использования:
    /// ```ignore
    /// for entry in driver.ls("/some/path")? {
    ///     println!("{:?}", entry);
    /// }
    /// ```
    fn ls<P>(&self, path: P) -> Result<impl Iterator<Item = PathBuf>, DriverError>
    where
        P: AsRef<Path> + Debug;

    /// Получить инофрмацию о файле/директории.
    ///
    /// @param path - Путь к файлу или директории.
    fn stat<P: AsRef<Path>>(&self, path: P) -> Result<Stat, DriverError>;

    /// Перемещает/переименовывает файл/директорию.
    ///
    /// @param from - Исходный путь.
    /// @param to - Целевой путь.
    /// @return Result<()> - Результат: успех или ошибка
    fn rename<P, Q>(&self, from: P, to: Q) -> Result<(), DriverError>
    where
        P: AsRef<Path> + Debug,
        Q: AsRef<Path> + Debug;

    /// Создает директорию.
    ///
    /// @param path - Путь к директории.
    /// @param recursive - Рекурсивное создание. Создает все промежуточные директории.
    /// @return Result<()> - Результат: успешное создание или ошибка.
    fn mkdir<P>(&self, path: P, recursive: bool) -> Result<(), DriverError>
    where
        P: AsRef<Path> + Debug;

    /// Удаляет директорию/файл.
    ///
    /// @param path - Путь к директории.
    /// @return Result<()> - Результат: успешное удаление или ошибка.
    fn rm<P>(&self, path: P) -> Result<(), DriverError>
    where
        P: AsRef<Path> + Debug;

    /// Записывает данные в файл. Есть несколько режимов записи.
    ///
    /// @param path - Путь к файлу.
    /// @param mode - Режим записи.
    /// @return Result<Box<dyn io::Write>> - Результат: успешная запись или ошибка.
    fn write<P>(&self, path: &P, mode: WriteMode) -> Result<Box<dyn io::Write>, DriverError>
    where
        P: AsRef<Path> + Debug;

    /// Записывает данные в файл в режиме перезаписи (WriteMode::Overwrite)
    ///
    /// @param path - Путь к файлу.
    /// @param data - Данные для записи.
    /// @return Result<()> - Результат: успешная запись или ошибка.
    fn write_all<P, D>(&self, path: &P, data: D) -> Result<(), DriverError>
    where
        P: AsRef<Path>,
        D: AsRef<[u8]>,
    {
        let path = path.as_ref();
        // Записываем строку в lock файл
        let mut lock_writer = self.write(&path, WriteMode::Overwrite)?;

        let data = data.as_ref();
        lock_writer
            .write_all(data)
            .map_err(|err| DriverError::WriteError {
                path: path.to_path_buf(),
                reason: err.to_string(),
            })?;
        lock_writer.flush().map_err(|err| DriverError::WriteError {
            path: path.to_path_buf(),
            reason: err.to_string(),
        })
    }

    /// Читает данные из файла.
    ///
    /// @param path - Путь к файлу.
    /// @param position - Позиция в файле, с которой начать чтение.
    /// @return Result<Box<dyn io::Read>> - Результат: успешное чтение или ошибка.
    fn read<P>(&self, path: &P, position: u64) -> Result<Box<dyn io::Read>, DriverError>
    where
        P: AsRef<Path> + Debug;

    /// Копирует файл.
    ///
    /// @param from - Путь к исходному файлу.
    /// @param to - Путь к целевому файлу.
    fn copy_file<P, Q>(&self, from: &P, to: &Q) -> Result<(), DriverError>
    where
        P: AsRef<Path> + Debug,
        Q: AsRef<Path> + Debug;

    /// Копирует файл/директорию.
    ///
    /// @param from - Путь к исходному файлу.
    /// @param to - Путь к целевому файлу.
    ///
    /// @return успех или ошибка.
    fn copy<P, Q>(&self, from: &P, to: &Q) -> Result<(), DriverError>
    where
        P: AsRef<Path> + Debug,
        Q: AsRef<Path> + Debug,
    {
        let from = from.as_ref();
        let to = to.as_ref();

        debug!(?from, ?to, "копируем from->to=");

        if !self.exists(from) {
            debug!(?from, "не существует");
            return Err(DriverError::PathNotExistsError(from.to_path_buf()));
        }

        let from_stat = self.stat(from)?;

        if matches!(from_stat, Stat::File { .. }) {
            debug!(?from, "копируем файл");
            return self.copy_file(&from.to_path_buf(), &to.to_path_buf());
        }

        if !self.exists(to) {
            debug!(?to, "создаем директорию");
            self.mkdir(to.to_path_buf(), false)?;
        }

        for from_in in self.ls(&from)? {
            let to_in = from_in
                .file_name()
                .map(|n| to.join(n))
                .ok_or_else(|| DriverError::FileNameError(from_in.clone()))?;

            self.copy(&from_in, &to_in)?;
        }

        Ok(())
    }

    /// Обновляет время последнего изменения файла на текущее.
    ///
    /// @param path - Путь к файлу.
    ///
    /// @return успех или ошибка.
    fn update_file_modified_time_now(&self, path: impl AsRef<Path>) -> Result<(), DriverError>;
}
