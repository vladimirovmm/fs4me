use fs4me_interface::{Driver, DriverError, Stat, WriteMode};
use std::{
    fmt::Debug,
    io,
    path::{Path, PathBuf},
};
use tracing::error;

pub(crate) mod lock;
pub(crate) mod trash;
pub(crate) mod uuid;

use crate::{
    lock::{LockMode, is_operation_allowed},
    trash::trash_unique_path,
    uuid::FsUuid,
};

/// Обёртка для драйвера для безопасного доступа к файловой системе.
/// Обёртка обеспечивает безопасный одновременный доступ к файлу через lock файл.
#[derive(Debug, Clone)]
pub struct Fs<D: Driver> {
    /// Драйвер для доступа к файловой системе.
    driver: Box<D>,
    /// Индификатор подключения. Нужен для работы с lock файлами.
    uuid: FsUuid,
}

impl<D: Driver> From<D> for Fs<D> {
    fn from(value: D) -> Self {
        Self::new(value)
    }
}

impl<D: Driver> Drop for Fs<D> {
    fn drop(&mut self) {
        if let Err(e) = self.driver.disconnect() {
            error!("Failed to disconnect: {e}");
        }
    }
}

impl<D: Driver> Fs<D> {
    pub fn new(driver: D) -> Self {
        Self {
            driver: Box::new(driver),
            uuid: FsUuid::default(),
        }
    }

    /// Возвращает информацию о драйвере.
    ///
    /// @return Строка с информацией о драйвере.
    pub fn driver_info(&self) -> String {
        self.driver.info()
    }

    /// Возвращает текущее время сервера.
    ///
    /// @return Возвращает `Ok` с текущим временем сервера в формате Unix timestamp, или `Err` в случае ошибки.
    pub fn time(&self) -> Result<u32, DriverError> {
        self.driver.server_time()
    }

    /// Проверяет существование файла или директории.
    ///
    /// @param path - Путь к файлу или директории.
    /// @return bool - Результат: true, если файл или директория существует, false - если нет.
    pub fn exists<P: AsRef<Path>>(&self, path: P) -> bool {
        self.driver.exists(path)
    }

    /// Возвращает список файлов и директорий в указанной директории.
    ///
    /// @param path - Путь к директории.
    /// @return Возвращает `Ok` с итератором по `PathBuf`, или `Err` в случае ошибки.
    pub fn ls<P: AsRef<Path>>(
        &self,
        path: P,
    ) -> Result<impl Iterator<Item = PathBuf>, DriverError> {
        self.driver.ls(path)
    }

    /// Возвращает информацию о файле или директории.
    ///
    /// @param path - Путь к файлу или директории.
    /// @return Возвращает `Ok` с информацией о файле или директории, или `Err` в случае ошибки.
    pub fn stat<P: AsRef<Path>>(&self, path: P) -> Result<Stat, DriverError> {
        // @todo добавить сюда информацию о блокировках, сколько читают, пишут...
        self.driver.stat(path)
    }

    /// Перемещает/переименовывает файл/директорию.
    /// Можно перемещать/переименовывать только если у файла/директории нет читателя или писателя
    ///
    /// @param from - Исходный путь.
    /// @param to - Целевой путь.
    /// @return Result<()> - Результат: успех или ошибка
    pub fn mv<P: AsRef<Path>, Q: AsRef<Path>>(&self, from: P, to: Q) -> Result<(), DriverError> {
        let to = to.as_ref();

        if !is_operation_allowed(self, to, LockMode::Write)? {
            return Err(DriverError::LockedForWriteError {
                path: to.to_path_buf(),
                reason: "Путь заблокирован для перемещения".to_string(),
            });
        }

        self.driver.mv(from, to)
    }

    /// Создает директорию.
    /// Без проверки блокировок.
    ///
    /// @param path - Путь к директории.
    /// @param recursive - Если `true`, то создается вся цепочка директорий.
    pub fn mkdir<P: AsRef<Path>>(&self, path: P, recursive: bool) -> Result<(), DriverError> {
        self.driver.mkdir(path, recursive)
    }

    /// Перемещает указанный файл или директорию в корзину.
    ///
    /// Проверка блокировок осуществляется только для самого удаляемого пути; блокировки вложенных элементов
    /// (если это директория) не проверяются. Сама проверка осуществляется в методе `mv`.
    ///
    /// @param path - Путь к удаляемому файлу или директории.
    /// @return Result<()> - Результат: успешное удаление (перемещение в корзину) или ошибка.
    pub fn rm<P: AsRef<Path>>(&self, path: P) -> Result<(), DriverError> {
        let path = path.as_ref();
        let new_path = trash_unique_path(self.driver.as_ref(), path)?;

        self.mv(path, new_path)
    }

    /// Записывает данные в файл. Есть несколько режимов записи.
    ///
    /// @param path - Путь к файлу.
    /// @param mode - Режим записи.
    /// @return Result<Box<dyn io::Write>> - Результат: успешная запись или ошибка.
    pub fn write<P: AsRef<Path>>(
        &self,
        path: &P,
        mode: WriteMode,
    ) -> Result<Box<dyn io::Write>, DriverError> {
        // @todo проверить блокировку
        self.driver.write(path, mode)
    }

    /// Читает данные из файла.
    ///
    /// @param path - Путь к файлу.
    /// @param position - Позиция в файле, с которой начать чтение.
    /// @return Result<Box<dyn io::Read>> - Результат: успешное чтение или ошибка.
    pub fn read<P: AsRef<Path>>(
        &self,
        path: &P,
        position: u64,
    ) -> Result<Box<dyn io::Read>, DriverError> {
        // @todo проверить блокировку
        self.driver.read(path, position)
    }
}

/// Вернуть идентификатор клиента.
impl<D: Driver> AsRef<FsUuid> for Fs<D> {
    fn as_ref(&self) -> &FsUuid {
        &self.uuid
    }
}
