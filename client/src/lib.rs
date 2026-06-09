use fs4me_interface::{Driver, DriverError, Stat, WriteMode};
use fs4me_lock::{LockMode, MultiLock};
use fs4me_uuid::FsUuid;
use std::{
    fmt::Debug,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tracing::{debug, error, instrument};

pub mod buffer;
pub(crate) mod trash;

use crate::{
    buffer::{DriverBufferRead, DriverBufferWrite},
    trash::trash_unique_path,
};

/// Обёртка для драйвера для безопасного доступа к файловой системе.
/// Обёртка обеспечивает безопасный одновременный доступ к файлу через lock файл.
#[derive(Debug)]
pub struct Fs<D: Driver> {
    /// Драйвер для доступа к файловой системе.
    pub driver: Arc<D>,
    /// Индификатор подключения. Нужен для работы с lock файлами.
    pub uuid: FsUuid,
}

impl<D: Driver> Clone for Fs<D> {
    fn clone(&self) -> Self {
        Self {
            driver: self.driver.clone(),
            uuid: self.uuid.new_copy_id(),
        }
    }
}

/// Вернуть идентификатор клиента.
impl<D: Driver> AsRef<FsUuid> for Fs<D> {
    fn as_ref(&self) -> &FsUuid {
        &self.uuid
    }
}

/// Преобразование из драйвера в клиента
impl<D: Driver> From<D> for Fs<D> {
    fn from(value: D) -> Self {
        Self::new(value)
    }
}

/// Автоматическое отключение от сервера при удалении клиента
impl<D: Driver> Drop for Fs<D> {
    fn drop(&mut self) {
        if let Err(e) = self.driver.disconnect() {
            error!("Failed to disconnect: {e}");
        }
    }
}

impl<D: Driver> Fs<D> {
    /// Создает новый клиент, предоставляя доступ к драйверу файловой системы.
    ///
    /// @param driver - экземпляр драйвера, используемый для взаимодействия с файловой системой.
    /// @return Fs<D> - новый инициализированный экземпляр клиента.
    #[instrument(level = "debug", skip_all)]
    pub fn new(driver: D) -> Self {
        Self {
            // Драйвер для доступа к файловой системе.
            driver: Arc::new(driver),
            // Уникальный идентификатор клиента + номер клона. При каждом клонировании номер клона будет инкрементироваться.
            uuid: FsUuid::default(),
        }
    }

    /// Возвращает информацию о драйвере.
    ///
    /// @return Строка с информацией о драйвере.
    #[instrument(level = "debug", skip_all)]
    pub fn driver_info(&self) -> String {
        self.driver.info()
    }

    /// Возвращает текущее время сервера.
    ///
    /// @return Возвращает `Ok` с текущим временем сервера в формате Unix timestamp, или `Err` в случае ошибки.
    #[instrument(level = "debug", skip(self))]
    pub fn time(&self) -> Result<Duration, DriverError> {
        self.driver.time()
    }

    /// Проверяет существование файла или директории.
    ///
    /// @param path - Путь к файлу или директории.
    /// @return bool - Результат: true, если файл или директория существует, false - если нет.
    #[instrument(level = "debug", skip(self))]
    pub fn exists<P>(&self, path: P) -> bool
    where
        P: AsRef<Path> + Debug,
    {
        self.driver.exists(path)
    }

    /// Возвращает список файлов и директорий в указанной директории.
    ///
    /// @param path - Путь к директории.
    /// @return Возвращает `Ok` с итератором по `PathBuf`, или `Err` в случае ошибки.
    #[instrument(level = "debug", skip(self))]
    pub fn ls<P>(&self, path: P) -> Result<impl Iterator<Item = PathBuf>, DriverError>
    where
        P: AsRef<Path> + Debug,
    {
        self.driver.ls(path)
    }

    /// Возвращает информацию о файле или директории.
    ///
    /// @param path - Путь к файлу или директории.
    /// @return Возвращает `Ok` с информацией о файле или директории, или `Err` в случае ошибки.
    pub fn stat<P: AsRef<Path>>(&self, path: P) -> Result<Stat, DriverError> {
        self.driver.stat(path)
    }

    /// Перемещает/переименовывает файл/директорию.
    /// Можно перемещать/переименовывать только если у файла/директории нет читателя или писателя
    ///
    /// @param from - Исходный путь.
    /// @param to - Целевой путь.
    /// @return Result<()> - Результат: успех или ошибка
    ///
    #[instrument(level = "debug", skip(self))]
    pub fn rename<P, Q>(&self, from: P, to: Q) -> Result<(), DriverError>
    where
        P: AsRef<Path> + Debug,
        Q: AsRef<Path> + Debug,
    {
        let from = from.as_ref();
        let to = to.as_ref();

        // Блокируем исходный и целевой файлы/директории для записи
        // Проверка на наличие родительской директории происходит в блокировке
        // Разблокируется автоматически по выходе из области видимости
        debug!(?from, "Блокируем");
        let _from_lock =
            MultiLock::try_lock(self.uuid, self.driver.clone(), from, LockMode::Write)?;
        debug!(?to, "Блокируем");
        let _to_lock = MultiLock::try_lock(self.uuid, self.driver.clone(), to, LockMode::Write)?;

        // Перемещаем файл/директорию
        debug!("Перемещаем from->to");
        self.driver.rename(from, to)?;

        Ok(())
    }

    /// Создает директорию.
    /// Без проверки блокировок.
    ///
    /// @param path - Путь к директории.
    /// @param recursive - Если `true`, то создается вся цепочка директорий.
    #[instrument(level = "debug", skip(self))]
    pub fn mkdir<P>(&self, path: P, recursive: bool) -> Result<(), DriverError>
    where
        P: AsRef<Path> + Debug,
    {
        let path = path.as_ref();
        let _lock_to = MultiLock::try_lock(self.uuid, self.driver.clone(), path, LockMode::Write)?;
        self.driver.mkdir(path, recursive)
    }

    /// Перемещает указанный файл или директорию в корзину.
    ///
    /// Проверка блокировок осуществляется только для самого удаляемого пути; блокировки вложенных элементов
    /// (если это директория) не проверяются. Сама проверка осуществляется в методе `mv`.
    ///
    /// @param path - Путь к удаляемому файлу или директории.
    /// @return Result<()> - Результат: успешное удаление (перемещение в корзину) или ошибка.
    #[instrument(level = "debug", skip(self))]
    pub fn rm<P>(&self, path: P) -> Result<(), DriverError>
    where
        P: AsRef<Path> + Debug,
    {
        let path = path.as_ref();
        // Путь, куда будет перемещён файл
        let new_path = trash_unique_path(self.driver.as_ref(), path)?;
        // Проверка блокировки происходит внутри mv
        self.rename(path, new_path)
    }

    /// Записывает данные в файл. Есть несколько режимов записи.
    ///
    /// @param path - Путь к файлу.
    /// @param mode - Режим записи.
    /// @return Result<Box<dyn io::Write>> - Результат: успешная запись или ошибка.
    #[instrument(level = "debug", skip(self))]
    pub fn write<P>(
        &self,
        path: &P,
        mode: WriteMode,
    ) -> Result<Box<DriverBufferWrite<D>>, DriverError>
    where
        P: AsRef<Path> + Debug,
    {
        // Нужно проверить что новый путь для перемещения.
        let path = &path.as_ref().to_path_buf();

        // Блокируем файл для записи.
        // Проверка на наличие родительской директории происходит внутри функции Lock.
        // Разблокируется автоматически по выходе из области видимости.
        let lock = MultiLock::try_lock(self.uuid, self.driver.clone(), path, LockMode::Write)?;

        self.driver
            .write(path, mode)
            .map(|write| Box::new(DriverBufferWrite { lock, write }))
    }

    /// Читает данные из файла.
    ///
    /// @param path - Путь к файлу.
    /// @param position - Позиция в файле, с которой начать чтение.
    /// @return Result<Box<dyn io::Read>, DriverError> - Результат: успешное чтение или ошибка.
    #[instrument(level = "debug", skip(self))]
    pub fn read<P>(&self, path: &P, position: u64) -> Result<Box<DriverBufferRead<D>>, DriverError>
    where
        P: AsRef<Path> + Debug,
    {
        // Нужно проверить что новый путь для перемещения.
        let path = &path.as_ref().to_path_buf();

        // Блокируем файл для чтения.
        // Проверка на наличие родительской директори происход внутри функции Lock.
        // Разблокируется автоматически по выходе из области видимости
        let lock = MultiLock::try_lock(self.uuid, self.driver.clone(), path, LockMode::Read)?;

        self.driver
            .read(path, position)
            .map(|read| Box::new(DriverBufferRead { lock, read }))
    }

    /// Копирует файл.
    ///
    /// @param from - Путь к исходному файлу.
    /// @param to - Путь к целевому файлу.
    pub fn copy_file<P, Q>(&self, from: &P, to: &Q) -> Result<(), DriverError>
    where
        P: AsRef<Path> + Debug,
        Q: AsRef<Path> + Debug,
    {
        let _lock_from = MultiLock::try_lock(self.uuid, self.driver.clone(), from, LockMode::Read)?;
        let _lock_to = MultiLock::try_lock(self.uuid, self.driver.clone(), to, LockMode::Write)?;

        self.driver.copy_file(from, to)
    }

    /// Копирует файл/директорию.
    ///
    /// @param from - Путь к исходному файлу.
    /// @param to - Путь к целевому файлу.
    ///
    /// @return успех или ошибка.
    pub fn copy<P, Q>(&self, from: &P, to: &Q) -> Result<(), DriverError>
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

        let _lock_from = MultiLock::try_lock(self.uuid, self.driver.clone(), from, LockMode::Read)?;

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
}
