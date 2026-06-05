use fs4me_interface::Driver;
use fs4me_local::LocalDriver;
use fs4me_lock::{
    LockMode, MultiLock,
    base_lock::{
        BaseLock,
        paths::{base_lock_path, multi_lock_path},
    },
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
#[cfg_attr(not(feature = "test_env"), ignore)]
fn test_base_lock_timeout() {
    // let Init {
    //     driver,
    //     uuid,
    //     tmp: _tmp,
    //     source_path,
    // } = Default::default();

    // let LockPaths {
    //     base: base_path, ..
    // } = (&source_path).try_into().unwrap();

    // let base_lock_a = BaseLock::try_lock(uuid, driver.clone(), source_path.clone()).unwrap();
    // info!(?base_lock_a.path, "Пишем значение файла до блокировки");
    // fs::write(&base_lock_a.path, "null").unwrap();

    // info!("Блокируем файл");
    // let _lock_a = base_lock_a.try_lock().unwrap();

    // info!(?base_lock_a.tmp_path, "Пишем значение файла после блокировки");
    // fs::write(&base_lock_a.tmp_path, "a").unwrap();

    // // Создаём новый lock что бы были разные tmp
    // let base_lock_b = BaseLock::try_form(uuid, driver, source_path).unwrap();

    // assert!(
    //     base_lock_b.try_lock().is_err(),
    //     "Файл должен быть заблокирован"
    // );

    // info!("Подождём, когда блокировка устареет");
    // sleep(Duration::from_secs(5));

    // info!("Попробуем установить новую блокировку");
    // let _lock_b = base_lock_b.try_lock().unwrap();
    // fs::write(&base_lock_b.tmp_path, "b").unwrap();
    // drop(_lock_b);
    // drop(_lock_a);

    // info!("Ждем снятие всех блокировок");
    // sleep(Duration::from_secs(1));

    // info!(
    //     "Проверяем что значение в файле должно быть `b` так как `a` утратило владение из-за timeout."
    // );
    // let content = fs::read_to_string(&base_lock_b.path).unwrap();

    // assert_eq!(&content, "b");
}
