use std::{
    thread::{self, sleep},
    time::Duration,
};

use fs4me_lock::{LockMode, MultiLock};
use tracing::info;
use tracing_test::traced_test;

use crate::init::{Init, read_lock};

mod init;

/// Тест на блокировку при параллельном чтении
#[test]
#[traced_test]
fn test_lock() {
    let Init {
        driver,
        uuid,
        tmp: _tmp,
        src,
    } = Default::default();

    let count = 10;
    let _locks = (0..count)
        .map(|num| {
            info!(?num, "===== Start =====");

            let result =
                MultiLock::try_from(uuid.new_copy_id(), driver.clone(), &src, LockMode::Read)
                    .unwrap();

            info!(?num, "===== End =====");
            result
        })
        .collect::<Vec<_>>();

    let (lock_content, lock_count_in_file) = read_lock(&src);
    info!(?lock_count_in_file, "Содержимое файла: \n{lock_content}");
    assert_eq!(count, lock_count_in_file);
}

/// Тест на блокировку при наличии параллельных читателей и писателей
#[test]
#[traced_test]
fn test_concurrent_read_blocks_write() {
    let Init {
        driver,
        uuid,
        tmp: _tmp,
        src,
    } = Default::default();

    info!("Создаем читателей");
    let count = 2;
    let read_locks = (0..count)
        .map(|_| {
            MultiLock::try_from(uuid.new_copy_id(), driver.clone(), &src, LockMode::Read).unwrap()
        })
        .collect::<Vec<_>>();

    let (lock_content, lock_count_in_file) = read_lock(&src);
    info!(?lock_count_in_file, "Содержимое файла: \n{lock_content}");
    assert_eq!(count, lock_count_in_file);

    info!("Попытка перейти в режим записи");
    let source_path = src.clone();
    let driver_write = driver.clone();
    let write_lock = thread::spawn(move || {
        MultiLock::try_from(
            uuid.new_copy_id(),
            driver_write,
            source_path,
            LockMode::Write,
        )
        .unwrap()
    });

    info!("Ждем пока запись встанет в очередь");
    sleep(Duration::from_secs(1));

    let (lock_content, lock_count_in_file) = read_lock(&src);
    info!(?lock_count_in_file, "Содержимое файла: \n{lock_content}");
    assert_eq!(3, lock_count_in_file);
    assert!(lock_content.contains(&LockMode::WriteQueue.to_string()));

    info!("Новый читатель не может встать, пока есть очередь на запись");
    let source_path = src.clone();
    let driver_read = driver;
    let new_read_lock = thread::spawn(move || {
        MultiLock::try_from(uuid.new_copy_id(), driver_read, source_path, LockMode::Read).unwrap()
    });

    info!("Ждем секунду, чтобы убедиться, что не появились новые читатели");
    sleep(Duration::from_secs(1));

    let (lock_content, lock_count_in_file) = read_lock(&src);
    info!(?lock_count_in_file, "Содержимое файла: \n{lock_content}");
    assert_eq!(
        3, lock_count_in_file,
        "должно быть по прежнему 2 читателя и 1 в очереди на запись"
    );

    info!("Завершаем все чтения");
    drop(read_locks);

    info!("Ждем блокировки на запись");
    let write_lock = write_lock.join().unwrap();

    let (lock_content, lock_count_in_file) = read_lock(&src);
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
    let (lock_content, lock_count_in_file) = read_lock(&src);
    info!(?lock_count_in_file, "Содержимое файла: \n{lock_content}");
    assert_eq!(1, lock_count_in_file, "должно быть 1 блокировка на запись");
    assert!(
        lock_content.contains(&LockMode::Read.to_string()),
        "Должна быть блокировка на чтение"
    );
}
