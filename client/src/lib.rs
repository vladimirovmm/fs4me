use fs4me_interface::{Driver, DriverError, Stat, WriteMode};
use std::{
    fmt::Debug,
    io,
    path::{Path, PathBuf},
    str::FromStr,
    thread::sleep,
    time::{Duration, Instant},
};
use tracing::error;

pub(crate) mod lock;
pub(crate) mod trash;
pub(crate) mod uuid;

use crate::{
    lock::{LockInfo, LockMode, lock_path},
    trash::trash_unique_path,
    uuid::FsUuid,
};

fn parent_dir(path: &Path) -> Result<&Path, DriverError> {
    path.parent()
        .ok_or_else(|| DriverError::ParentDirError(path.to_path_buf()))
}

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
        self.driver.stat(path)
    }

    fn parent_dir_mast_exists<P: AsRef<Path>>(&self, path: P) -> Result<(), DriverError> {
        parent_dir(path.as_ref()).and_then(|path| {
            if self.exists(path) {
                Ok(())
            } else {
                Err(DriverError::ParentDirError(path.to_path_buf()))
            }
        })
    }

    /// Возвращает информацию о блокировке файла или директории.
    ///
    /// @param path - Путь к файлу или директории.
    /// @return Возвращает `Ok` с информацией о блокировке, или `Err` в случае ошибки.
    fn read_lock<P: AsRef<Path>>(&self, path: P) -> Result<LockInfo, DriverError> {
        let lock_file = lock_path(path)?;
        if !self.exists(&lock_file) {
            // Если lock файл не существует, возвращаем пустую структуру LockStat
            return Ok(LockInfo::default());
        }
        // Читаем содержимое lock файла
        let lock_reader = self.driver.read(&lock_file, 0)?;
        let lock_content =
            io::read_to_string(lock_reader).map_err(|err| DriverError::ReadSeekError {
                path: lock_file.to_path_buf(),
                reason: err.to_string(),
            })?;
        // Парсим содержимое lock файла в структуру LockStat
        let mut lock_stat = LockInfo::from_str(&lock_content)?;
        // Удаляем устаревшие блокировки (unixtime + 5 минут < now)
        lock_stat.remove_stale(self.time()?);

        Ok(lock_stat)
    }

    /// Записывает информацию о блокировке файла или директории.
    ///
    /// @param path - Путь к файлу или директории.
    /// @param lock - Информация о блокировке.
    /// @return Возвращает `Ok` в случае успеха, или `Err` в случае ошибки.
    fn write_lock<P: AsRef<Path>>(&self, path: P, lock: LockInfo) -> Result<(), DriverError> {
        let lock_file = lock_path(path)?;
        // Преобразуем структуру LockStat в строку
        let lock_content = lock.to_string();
        // Записываем строку в lock файл
        let mut lock_writer = self.driver.write(&lock_file, WriteMode::Overwrite)?;
        lock_writer
            .write_all(lock_content.as_bytes())
            .map_err(|err| DriverError::WriteError {
                path: lock_file,
                reason: err.to_string(),
            })
    }

    /// Пытается блокировать файл/директорию для чтения/записи.
    ///
    /// @param path - Путь к файлу или директории.
    /// @param mode - Режим блокировки: `Read`, `Write` и `WriteQueue`.
    /// @return Result<()> - Результат: успех или ошибка
    pub fn try_lock<P: AsRef<Path>>(&self, path: P, mode: LockMode) -> Result<(), DriverError> {
        let path = path.as_ref();

        self.parent_dir_mast_exists(path)?;

        if matches!(mode, LockMode::Write) {
            self.try_lock(path, LockMode::WriteQueue)?;
        }

        let mut lock = self.read_lock(path)?;
        lock.set(self.uuid, self.time()?, mode)
            .map_err(|_| DriverError::LockedError {
                path: path.to_path_buf(),
                mode: mode.to_string(),
            })?;
        self.write_lock(path, lock)
    }

    /// Попытка блокировки файла/директории для чтения/записи в течение 30 секунд.
    ///
    /// @param path - Путь к файлу или директории.
    /// @param mode - Режим блокировки: `Read`, `Write` и `WriteQueue`.
    /// @return Result<()> - Результат: успех или ошибка
    pub fn lock<P: AsRef<Path>>(&self, path: P, mode: LockMode) -> Result<(), DriverError> {
        let start = Instant::now();
        let path = path.as_ref();

        self.parent_dir_mast_exists(path)?;

        loop {
            let result = self.try_lock(path, mode);
            // Либо успех, либо время вышло
            if result.is_ok() || start.elapsed() > Duration::from_secs(30) {
                return result;
            }

            sleep(Duration::from_millis(250));
        }
    }

    /// Попытка снять блокировку от имени текущего uuid.
    ///
    /// @param path - Путь к файлу/директории.
    /// @return Result<()> - Результат: успех или ошибка
    pub fn try_unlock<P: AsRef<Path>>(&self, path: P) -> Result<(), DriverError> {
        let path = path.as_ref();

        self.parent_dir_mast_exists(path)?;

        let mut lock = self.read_lock(path)?;
        lock.remove(self);
        self.write_lock(path, lock)
    }

    /// Снять блокировку от имени текущего uuid.
    ///
    /// @param path - Путь к файлу/директории.
    /// @return Result<()> - Результат: успех или ошибка
    pub fn unlock<P: AsRef<Path>>(&self, path: P) -> Result<(), DriverError> {
        let start = Instant::now();

        let path = path.as_ref();
        self.parent_dir_mast_exists(path)?;

        loop {
            let result = self.try_unlock(path);
            // Либо успех, либо время вышло
            if result.is_ok() || start.elapsed() > Duration::from_secs(30) {
                return result;
            }

            sleep(Duration::from_millis(250));
        }
    }

    /// Перемещает/переименовывает файл/директорию.
    /// Можно перемещать/переименовывать только если у файла/директории нет читателя или писателя
    ///
    /// @param from - Исходный путь.
    /// @param to - Целевой путь.
    /// @return Result<()> - Результат: успех или ошибка
    pub fn mv<P: AsRef<Path>, Q: AsRef<Path>>(&self, from: P, to: Q) -> Result<(), DriverError> {
        // Нужно проверить что новый путь для перемещения.
        let to = to.as_ref();
        let parent_to = to.parent().unwrap_or(to);
        if !self.exists(parent_to) {
            return Err(DriverError::ParentDirError(parent_to.to_path_buf()));
        }

        // Блокируем исходный и целевой файлы/директории для записи
        let from = from.as_ref();
        self.lock(from, LockMode::Write)?;
        self.lock(to, LockMode::Write)?;
        self.driver.mv(from, to)?;
        self.unlock(from)?;
        self.unlock(to)?;

        Ok(())
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
