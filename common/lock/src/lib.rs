use fs4me_interface::{Driver, DriverError};
use fs4me_uuid::FsUuid;
use std::{
    fmt::{Debug, Display},
    path::{Path, PathBuf},
    str::FromStr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle, sleep},
    time::{Duration, Instant},
};
use tracing::{debug, error, instrument, warn};

pub mod base_lock;
pub mod helpers;
pub(crate) mod lock_info;
pub use crate::lock_info::LockInfo;

use crate::{
    base_lock::{BaseLock, paths::multi_lock_path},
    helpers::background_refresh_interval,
    lock_info::LockInfoRead,
};

/// Повторяет операции блокировки/разблокировки с экспоненциальной задержкой.
/// Максимальное время ожидания: 30 секунд.
///
/// @param retry_fn - Функция, которая будет повторяться.
/// @returns Результат повторений.
#[instrument(level = "debug", skip_all)]
fn retry<F>(mut retry_fn: F) -> Result<(), DriverError>
where
    F: FnMut() -> Result<(), DriverError>,
{
    // Время начала отсчета
    let start = Instant::now();
    // Интервал между повторами
    let mut interval = Duration::from_millis(50);
    // Максимальное время повторений
    let limit_secs = Duration::from_secs(30);

    loop {
        let result = retry_fn();
        debug!(?result);

        // Не повторяем попытки при фатальных ошибках:
        // - Отсутствует родительская директория
        // - Не удалось получить имя файла
        if let Err(err) = &result
            && matches!(
                err,
                DriverError::ParentDirError(..) | DriverError::FileNameError(..)
            )
        {
            return result;
        }

        // Либо успех, либо время вышло
        if result.is_ok() || start.elapsed() > limit_secs {
            return result;
        }

        // При превышении максимальной задержки используем фиксированный интервал
        interval = interval.saturating_mul(2).min(Duration::from_secs(1));

        let jitter = Duration::from_millis(rand::random_range(0..250));
        sleep(interval + jitter);
    }
}

pub struct MultiLock<D: Driver> {
    /// Уникальный идентификатор клиента.
    uuid: FsUuid,
    /// Драйвер для работы с файловой системой.
    driver: Arc<D>,
    /// Путь к блокируемому файлу или директории.
    source_path: PathBuf,
    /// Хеш содержимого блокировки.
    hash: Option<u64>,
    /// Время последнего изменения блокировки.
    modified_time: Option<Duration>,
    /// Путь до файла блокировки.
    lock_path: PathBuf,
    /// Режим блокировки.
    mode: LockMode,
    /// Флаг остановки потока обновления блокировки.
    stop_refresh: Arc<AtomicBool>,
    /// Поток обновления блокировки.
    refresh_handle: Option<JoinHandle<Result<(), DriverError>>>,
}

impl<D: Driver> Display for MultiLock<D> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Lock - uuid: {}, path: {:?}",
            self.uuid, self.source_path
        )
    }
}

impl<D: Driver> MultiLock<D> {
    /// Блокирует файл или директорию для чтения или записи.
    ///
    /// @param uuid - Уникальный идентификатор клиента.
    /// @param driver - Драйвер для работы с файловой системой.
    /// @param path - Путь к файлу или директории.
    /// @param mode - Режим блокировки.
    /// @returns `Ok(MultiLock)` в случае успеха, `Err(DriverError)` при ошибке.
    #[instrument(level = "debug", skip(driver))]
    pub fn try_lock<P>(
        uuid: FsUuid,
        driver: Arc<D>,
        path: P,
        mode: LockMode,
    ) -> Result<Self, DriverError>
    where
        P: AsRef<Path> + Debug,
    {
        let source_path = path.as_ref().to_path_buf();
        let multi = multi_lock_path(&source_path).unwrap();
        let mut lock = Self {
            lock_path: multi,
            uuid,
            driver,
            hash: None,
            modified_time: None,
            source_path,
            mode,
            stop_refresh: Arc::new(AtomicBool::new(false)),
            refresh_handle: None,
        };

        lock.retry_lock()?;

        lock.refresh_handle = Some(lock.background_lock_refresh());

        Ok(lock)
    }

    /// Считывает информацию о текущей блокировке.
    /// Удаляет устаревшие блокировки и вычисляет хеш содержимого.
    ///
    /// @returns `Ok(LockInfoRead)` с информацией о блокировке или пустой структурой,
    /// если файл блокировки не существует. `Err(DriverError)` при ошибке чтения.
    #[instrument(level = "debug", skip(self))]
    fn read(&self) -> Result<LockInfoRead, DriverError> {
        if !self.driver.exists(&self.lock_path) {
            debug!("Блокировка не существует, возвращаем пустую информацию");
            // Если lock файл не существует, возвращаем пустую структуру LockStat
            return Ok(LockInfoRead::default());
        }

        let path = &self.lock_path;

        debug!("Читаем файл блокировки");
        // Читаем содержимое lock файла
        let lock_content = self.driver.read_all_string(path)?;

        // Парсим содержимое lock файла в структуру LockStat
        let mut lock_info = LockInfo::from_str(&lock_content)?;
        // Удаляем устаревшие блокировки (unixtime + 5 минут < now)
        lock_info.remove_stale(self.driver.time()?);

        // Вычисляем хеш содержимого lock файла
        let hash = Some(lock_info.get_hash());
        // Получаем время последнего изменения lock файла
        let modified_time = self.driver.stat(path).ok().map(|stat| stat.modified());
        if modified_time.is_none() {
            // Если lock файл не существует, возвращаем пустую структуру LockStat
            return Ok(LockInfoRead::default());
        }

        Ok(LockInfoRead {
            lock_info,
            modified_time,
            hash,
        })
    }

    /// Записывает информацию о блокировке во временный файл.
    ///
    /// @param lock - Информация о блокировке.
    /// @returns `Ok(())` при успехе, `Err(DriverError)` при ошибке записи.
    #[instrument(level = "debug", skip(self))]
    fn write_lock_file(&self, lock: LockInfo) -> Result<(), DriverError> {
        let tmp_path = &self.lock_path;

        debug!(?self.uuid, ?tmp_path, ?lock, "Пишем в файл блокировки");
        // Записываем строку в lock файл
        self.driver.write_all(&tmp_path, lock.to_string())?;

        Ok(())
    }

    /// Удаляет файл блокировки, так как он пустой (нет активных блокировок).
    ///
    /// @returns `Ok(())` при успехе, `Err(DriverError)` при ошибке удаления.
    #[instrument(level = "debug", skip_all)]
    fn drop_lock_file(&self) -> Result<(), DriverError> {
        debug!("Удаляем файл блокировки потому что он пустой");
        // Удаляем файл блокировки
        if let Err(err) = self.driver.rm(&self.lock_path) {
            debug!(?err, "Ошибка при удалении файла блокировки");
        }

        debug!("Файл блокировки удален");
        Ok(())
    }

    /// Записывает информацию о блокировке. Удаляет файл, если список блокировок пуст.
    ///
    /// @param lock - Информация о блокировке.
    /// @returns `Ok(())` при успехе, `Err(DriverError)` при ошибке.
    #[instrument(level = "debug", skip_all)]
    fn write(&self, lock: LockInfo) -> Result<(), DriverError> {
        if lock.is_empty() {
            // Блокировок нет больше, удаляем файл блокировки
            return self.drop_lock_file();
        }
        // Обновляем запись о блокировках
        self.write_lock_file(lock)
    }

    /// Пытается установить блокировку (не в режиме ожидания).
    ///
    /// @param mode - Режим блокировки: `Read`, `Write` или `WriteQueue`.
    /// @returns `Ok(())` при успехе, `Err(DriverError)` если блокировка занята.
    #[instrument(level = "debug", skip(self))]
    fn inner_try_lock(&mut self, mode: LockMode) -> Result<(), DriverError> {
        if matches!(mode, LockMode::Write) {
            debug!("Перед блокировкой на запись нужно встать в очередь");
            self.inner_try_lock(LockMode::WriteQueue)?;
        }

        // Создаём уникальный доступ к lock файлу
        let _lock = BaseLock::try_lock(self.uuid, self.driver.clone(), &self.lock_path)?;
        debug!(?self.uuid, "Установлена блокировка");

        let LockInfoRead {
            mut lock_info,
            modified_time,
            hash,
        } = self.read()?;
        self.hash = hash;
        self.modified_time = modified_time;

        debug!(?lock_info, "ДО блокировки");
        lock_info
            .set(self.uuid, self.driver.time()?, mode)
            .inspect_err(|err| debug!(?self.uuid, ?err))
            .map_err(|_| DriverError::LockedError {
                path: self.source_path.clone(),
                mode: mode.to_string(),
            })?;
        debug!(?lock_info, "ПОСЛЕ");

        self.write(lock_info)
    }

    /// Снимает блокировку от имени текущего uuid.
    ///
    /// @returns `Ok(())` при успехе, `Err(DriverError)` при ошибке.
    #[instrument(level = "debug", skip(self))]
    fn inner_try_unlock(&mut self) -> Result<(), DriverError> {
        // Создаём уникальный доступ к lock файлу
        let _lock = BaseLock::try_lock(self.uuid, self.driver.clone(), &self.lock_path)?;

        let LockInfoRead {
            mut lock_info,
            modified_time,
            hash,
        } = self.read()?;
        self.hash = hash;
        self.modified_time = modified_time;

        debug!(?self.uuid, "Убираем uuid из списка блокировки");
        lock_info.remove(self.uuid);
        self.write(lock_info)
    }

    /// Регулярно обновляет время обновления файла блокировки, поддерживая актуальность блокировки.
    ///
    /// @returns `JoinHandle` для управления потоком обновления.
    fn background_lock_refresh(&self) -> JoinHandle<Result<(), DriverError>> {
        debug!("Инициализация потока обновления времени блокировки");

        let interval_thread = background_refresh_interval();
        let mut lock = self.clone_from_thread();
        thread::spawn(move || {
            let mut last = Instant::now();
            loop {
                if lock.stop_refresh.load(Ordering::SeqCst) {
                    break;
                }

                let elapsed = last.elapsed();
                if elapsed >= interval_thread {
                    lock.retry_lock()?;
                    last = Instant::now(); // сброс на текущее время
                }

                // Вместо thread::sleep() — park_timeout()
                let remaining = interval_thread - elapsed;
                thread::park_timeout(remaining);
            }

            Ok::<(), DriverError>(())
        })
    }
    /// Устанавливает блокировку с повторными попытками (макс. 30 сек).
    /// Использует стратегию экспоненциальной задержки с джиттером.
    ///
    /// @param mode - Режим блокировки: `Read`, `Write` или `WriteQueue`.
    /// @returns `Ok(())` при успехе, `Err(DriverError)` при ошибке.
    #[instrument(level = "debug", skip(self))]
    fn retry_lock(&mut self) -> Result<(), DriverError> {
        retry(|| -> Result<(), DriverError> {
            // Максимальное время ожидания
            self.inner_try_lock(self.mode)
        })
    }

    /// Снимает блокировку с повторными попытками (макс. 30 сек).
    /// Использует стратегию экспоненциальной задержки с джиттером.
    ///
    /// @returns `Ok(())` при успехе, `Err(DriverError)` при ошибке.
    #[instrument(level = "debug", skip(self))]
    fn retry_unlock(&mut self) -> Result<(), DriverError> {
        // Останавливаем поток обновления блокировки
        self.stop_refresh.store(true, Ordering::Relaxed);
        if let Some(handle) = self.refresh_handle.take() {
            handle.thread().unpark();
            if let Err(e) = handle.join() {
                error!(?e, ?self.uuid, "Ошибка при ожидании завершения автообновления блокировки");
            }
        }
        // Пытаемся снять блокировку
        retry(|| -> Result<(), DriverError> { self.inner_try_unlock() })
    }

    /// Возвращает копию структуры `MultiLock` с `refresh_handle` установленным в `None`.
    fn clone_from_thread(&self) -> Self {
        Self {
            uuid: self.uuid,
            driver: self.driver.clone(),
            source_path: self.source_path.clone(),
            hash: self.hash,
            modified_time: self.modified_time,
            lock_path: self.lock_path.clone(),
            mode: self.mode,
            stop_refresh: self.stop_refresh.clone(),
            refresh_handle: None,
        }
    }
}

impl<D: Driver> Drop for MultiLock<D> {
    fn drop(&mut self) {
        if let Err(e) = self.retry_unlock() {
            error!(?e, ?self.uuid, "Ошибка при снятии блокировки");
        }
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
