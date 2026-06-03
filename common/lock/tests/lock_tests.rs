use fs4me_interface::Driver;
use fs4me_local::LocalDriver;
use fs4me_lock::{
    LockMode, MultiLock,
    base_lock::{BaseLock, LockPaths},
};
use fs4me_uuid::FsUuid;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    thread::{self, sleep},
    time::Duration,
};
use tempfile::TempDir;
use tracing::info;
use tracing_test::traced_test;

fn read_lock(src: &Path) -> (String, usize) {
    let lock_path = LockPaths::try_from(src).unwrap().path;
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

            let result = MultiLock::try_from(
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
            MultiLock::try_from(
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
        MultiLock::try_from(uuid.new_copy_id(), driver_write, path, LockMode::Write).unwrap()
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
        MultiLock::try_from(uuid.new_copy_id(), driver_read, path, LockMode::Read).unwrap()
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

/// Тест на блокировку при параллельном чтении
#[test]
#[traced_test]
fn test_lock() {
    let Init {
        driver,
        uuid,
        tmp: _tmp,
        source_path,
    } = Default::default();

    let base_lock = BaseLock::try_form(uuid, driver.clone(), source_path).unwrap();
    let content = "test".to_string();
    {
        let _lock = base_lock.try_lock().unwrap();
        info!("Тестирование параллельной блокировки");

        assert!(base_lock.try_lock().is_err(), "Файл уже заблокирован");

        info!("Проверка путей во время блокировки");
        assert!(
            !driver.exists(&base_lock.path),
            "Файл должен быть перемещён в {:?}",
            base_lock.block_path
        );
        assert!(!driver.exists(&base_lock.tmp_path));
        assert!(
            driver.exists(&base_lock.block_path),
            "Файл должен быть перемещён в {:?}",
            base_lock.block_path
        );

        let mut writer = driver
            .write(
                &base_lock.tmp_path,
                fs4me_interface::WriteMode::FailIfExists,
            )
            .unwrap();
        write!(&mut writer, "{content}").unwrap();
        drop(writer);

        assert!(driver.exists(&base_lock.tmp_path));
    }

    info!("Проверка путей после разблокировки");
    assert!(
        !driver.exists(&base_lock.block_path),
        "Файл должен быть перемещён в {:?}",
        base_lock.block_path
    );
    assert!(
        driver.exists(&base_lock.path),
        "Файл должен быть перемещён в {:?}",
        base_lock.path
    );

    info!("Проверка перемещения tmp_path->path");
    assert!(
        !driver.exists(&base_lock.tmp_path),
        "Файл должен быть перемещён в {:?}",
        base_lock.tmp_path
    );

    let mut reader = driver.read(&base_lock.path, 0).unwrap();
    let mut buf = String::new();
    reader.read_to_string(&mut buf).unwrap();
    drop(reader);
    assert_eq!(content, buf);

    info!("Повторная блокировка");
    base_lock.try_lock().unwrap();
}

/// Тест на истечение времени блокировки, после которого следующий запрос на блокировку может быть выполнен
#[test]
fn test_lock_timeout() {
    unsafe {
        std::env::set_var("LOCK_TIMEOUT", "3");
    }

    let Init {
        driver,
        uuid,
        tmp: _tmp,
        source_path,
    } = Default::default();

    let base_lock = BaseLock::try_form(uuid, driver.clone(), source_path).unwrap();
    let _lock_a = base_lock.try_lock().unwrap();

    assert!(
        base_lock.try_lock().is_err(),
        "Файл должен быть заблокирован"
    );

    info!("Подождём, когда блокировка устареет");
    sleep(Duration::from_secs(5));

    info!("Попробуем установить новую блокировку");
    let _lock_b = base_lock.try_lock().unwrap();
}
