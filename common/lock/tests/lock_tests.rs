use fs4me_interface::Driver;
use fs4me_local::LocalDriver;
use fs4me_lock::{
    LockInfo, LockMode, MultiLock,
    base_lock::{
        BaseLock,
        paths::{base_lock_path, multi_lock_path},
    },
    helpers::time_expired,
};
use fs4me_uuid::FsUuid;
use std::{
    fs,
    path::{Path, PathBuf},
    str::FromStr,
    sync::Arc,
    thread::{self, sleep},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tempfile::TempDir;
use tracing::info;
use tracing_test::traced_test;

fn read_lock(src: &Path) -> (String, usize) {
    let lock_path = multi_lock_path(src).unwrap();
    let lock_content = fs::read_to_string(&lock_path).unwrap();
    let lock_count_in_file = lock_content.lines().count();

    (lock_content, lock_count_in_file)
}

struct Init {
    pub driver: Arc<LocalDriver>,
    pub uuid: FsUuid,
    pub tmp: TempDir,
    pub source_path: PathBuf,
}

impl Default for Init {
    fn default() -> Self {
        let driver = Arc::new(LocalDriver::connect("").unwrap());
        let uuid = FsUuid::default();

        let tmp = TempDir::with_prefix("test_lock_").unwrap();

        let root_path = tmp.path().to_path_buf();
        info!(?root_path);

        let src = root_path.join("src");
        info!(?src, "Директория для блокировки");

        driver.mkdir(&src, false).unwrap();

        Self {
            driver,
            uuid,
            tmp,
            source_path: src,
        }
    }
}

/// Тест на блокировку при параллельном чтении
#[test]
#[traced_test]
fn test_multi_lock() {
    let Init {
        driver,
        uuid,
        tmp: _tmp,
        source_path,
    } = Default::default();

    let count = 10;
    let _locks = (0..count)
        .map(|num| {
            info!(?num, "===== Start =====");

            let result = MultiLock::try_lock(
                uuid.new_copy_id(),
                driver.clone(),
                &source_path,
                LockMode::Read,
            )
            .unwrap();

            info!(?num, "===== End =====");
            result
        })
        .collect::<Vec<_>>();

    let (lock_content, lock_count_in_file) = read_lock(&source_path);
    info!(?lock_count_in_file, "Содержимое файла: \n{lock_content}");
    assert_eq!(count, lock_count_in_file);
}

/// Тест на блокировку при наличии параллельных читателей и писателей
#[test]
#[traced_test]
fn test_multi_lock_concurrent_read_blocks_write() {
    let Init {
        driver,
        uuid,
        tmp: _tmp,
        source_path,
    } = Default::default();

    info!("Создаем читателей");
    let count = 2;
    let read_locks = (0..count)
        .map(|_| {
            MultiLock::try_lock(
                uuid.new_copy_id(),
                driver.clone(),
                &source_path,
                LockMode::Read,
            )
            .unwrap()
        })
        .collect::<Vec<_>>();

    let (lock_content, lock_count_in_file) = read_lock(&source_path);
    info!(?lock_count_in_file, "Содержимое файла: \n{lock_content}");
    assert_eq!(count, lock_count_in_file);

    info!("Попытка перейти в режим записи");
    let path = source_path.clone();
    let driver_write = driver.clone();
    let write_lock = thread::spawn(move || {
        MultiLock::try_lock(uuid.new_copy_id(), driver_write, path, LockMode::Write).unwrap()
    });

    info!("Ждем пока запись встанет в очередь");
    sleep(Duration::from_secs(1));

    let (lock_content, lock_count_in_file) = read_lock(&source_path);
    info!(?lock_count_in_file, "Содержимое файла: \n{lock_content}");
    assert_eq!(3, lock_count_in_file);
    assert!(lock_content.contains(&LockMode::WriteQueue.to_string()));

    info!("Новый читатель не может встать, пока есть очередь на запись");
    let path = source_path.clone();
    let driver_read = driver;
    let new_read_lock = thread::spawn(move || {
        MultiLock::try_lock(uuid.new_copy_id(), driver_read, path, LockMode::Read).unwrap()
    });

    info!("Ждем секунду, чтобы убедиться, что не появились новые читатели");
    sleep(Duration::from_secs(1));

    let (lock_content, lock_count_in_file) = read_lock(&source_path);
    info!(?lock_count_in_file, "Содержимое файла: \n{lock_content}");
    assert_eq!(
        3, lock_count_in_file,
        "должно быть по прежнему 2 читателя и 1 в очереди на запись"
    );

    info!("Завершаем все чтения");
    drop(read_locks);

    info!("Ждем блокировки на запись");
    let write_lock = write_lock.join().unwrap();

    let (lock_content, lock_count_in_file) = read_lock(&source_path);
    info!(?lock_count_in_file, "Содержимое файла: \n{lock_content}");
    assert_eq!(1, lock_count_in_file, "должно быть 1 блокировка на запись");
    assert!(
        lock_content.contains(&LockMode::Write.to_string()),
        "Должна быть блокировка на запись"
    );
    assert!(
        !lock_content.contains(&LockMode::Read.to_string()),
        "Не должно быть ни одной блокировки на чтение. В момент записи можно только становиться в очередь на запись."
    );

    info!("Снимаем блокировку на записи");
    drop(write_lock);

    info!("Ждем блокировку на чтение");
    let _new_read_lock = new_read_lock.join().unwrap();
    let (lock_content, lock_count_in_file) = read_lock(&source_path);
    info!(?lock_count_in_file, "Содержимое файла: \n{lock_content}");
    assert_eq!(1, lock_count_in_file, "должно быть 1 блокировка на запись");
    assert!(
        lock_content.contains(&LockMode::Read.to_string()),
        "Должна быть блокировка на чтение"
    );
}

/// MultiLock - тестирование на обновление времени блокировки в фоне
#[test]
#[cfg_attr(not(feature = "test_env"), ignore)]
#[traced_test]
fn test_multi_lock_background_refresh() {
    let Init {
        driver,
        uuid,
        tmp: _tmp,
        source_path,
    } = Default::default();

    let multi_path = multi_lock_path(&source_path).unwrap();
    info!(?multi_path, "Файл блокировки");
    let modified = || -> Duration {
        let lock_content = fs::read_to_string(&multi_path).unwrap();
        LockInfo::from_str(&lock_content)
            .unwrap()
            .read
            .first()
            .cloned()
            .unwrap_or_default()
            .1
    };

    {
        let _lock = MultiLock::try_lock(uuid, driver, source_path, LockMode::Read).unwrap();

        let mut last = modified();
        for _ in 0..5 {
            sleep(Duration::from_secs(1));
            assert!(
                multi_path.exists(),
                "Блокировочный файл должен существовать"
            );
            let new_time = modified();
            info!("1");
            assert!(
                last >= new_time.saturating_sub(Duration::from_secs(3)),
                "Время блокировки должно обновляться в фоне"
            );
            info!("2");
            last = new_time;
        }
    }

    assert!(!multi_path.exists(), "Должна быть снята");
}

/// тестирование на устаревание блокировки (timeout)
#[test]
#[traced_test]
#[cfg_attr(not(feature = "test_env"), ignore)]
fn test_multi_lock_timeout() {
    let Init {
        driver,
        uuid,
        tmp: _tmp,
        source_path,
    } = Default::default();

    let multi_path = multi_lock_path(&source_path).unwrap();

    info!(?multi_path, "Имитируем активную блокировку");
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap() + Duration::from_secs(5);
    fs::write(&multi_path, format!("{uuid}={}=write", now.as_nanos())).unwrap();

    assert!(
        MultiLock::try_lock(
            uuid.new_copy_id(),
            driver.clone(),
            &source_path,
            LockMode::Read
        )
        .is_err(),
        "Блокировка в режиме записи. Чтение не разрешено"
    );

    info!("Имитируем блокировку с timeout");
    fs::write(
        &multi_path,
        format!(
            "{uuid}={}=read",
            now.saturating_sub(time_expired()).as_nanos()
        ),
    )
    .unwrap();

    MultiLock::try_lock(uuid.new_copy_id(), driver, source_path, LockMode::Read).unwrap();
}

/// Тест на блокировку при параллельном чтении
#[test]
#[traced_test]
fn test_base_lock() {
    let Init {
        driver,
        uuid,
        tmp: _tmp,
        source_path,
    } = Default::default();

    let base_path = base_lock_path(&source_path).unwrap();

    {
        let _lock = BaseLock::try_lock(uuid, driver.clone(), &source_path).unwrap();
        info!("Тестирование параллельной блокировки");

        info!("Попытка установить блокировку не дождавшись снятия блокировки");
        assert!(
            BaseLock::try_lock(uuid, driver.clone(), &source_path).is_err(),
            "Файл уже заблокирован"
        );
        info!("Выход из области видимости и снятия блокировки");
    }

    info!("Проверка путей после разблокировки");
    assert!(
        !driver.exists(&base_path),
        "Файл должен быть удалён {base_path:?}",
    );

    info!("=== end ===")
}

/// Тест на истечение времени блокировки, после которого следующий запрос на блокировку может быть выполнен
#[test]
#[traced_test]
fn test_base_lock_timeout() {
    let Init {
        driver,
        uuid,
        tmp: _tmp,
        source_path,
    } = Default::default();

    let base_path = base_lock_path(&source_path).unwrap();

    info!(?base_path, "Имитируем уже заблокированный файл");
    fs::write(&base_path, uuid.new_copy_id().to_string()).unwrap();

    info!(
        ?base_path,
        "Сейчас у него текущее время последнего изменения. Поэтому другие блокировки не могут быть выполнены"
    );

    assert!(
        BaseLock::try_lock(uuid, driver.clone(), source_path.clone()).is_err(),
        "Блокировка не должна быть выполнена"
    );
    assert!(base_path.exists(), "Блокировочный файл должен существовать");

    info!(
        ?base_path,
        "Устанавливаем время последней модификации -10мин от текущего"
    );
    let file = fs::File::open(&base_path).unwrap();
    file.set_modified(SystemTime::now() - time_expired())
        .unwrap();

    info!(?base_path, "Время блокировки истекло");
    BaseLock::try_lock(uuid, driver, source_path).unwrap();

    info!(?base_path, "Успешно заблокировано");
}

// тестирование на обновление времени блокировки в фоне
#[test]
#[cfg_attr(not(feature = "test_env"), ignore)]
#[traced_test]
fn test_base_lock_background_refresh() {
    let Init {
        driver,
        uuid,
        tmp: _tmp,
        source_path,
    } = Default::default();

    let base_path = base_lock_path(&source_path).unwrap();
    let modified = || -> Duration {
        fs::metadata(&base_path)
            .unwrap()
            .modified()
            .unwrap()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
    };

    {
        let _lock = BaseLock::try_lock(uuid, driver, source_path).unwrap();

        let mut last = modified();
        for _ in 0..5 {
            sleep(Duration::from_secs(1));
            assert!(base_path.exists(), "Блокировочный файл должен существовать");
            let new_time = modified();
            assert!(
                last >= new_time.saturating_sub(Duration::from_secs(3)),
                "Время блокировки должно обновляться в фоне"
            );
            last = new_time;
        }
    }

    assert!(!base_path.exists(), "Должна быть снята");
}
