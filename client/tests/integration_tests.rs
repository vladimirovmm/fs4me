use rand::{RngExt, distr::Alphanumeric};
use std::time::SystemTime;
use tracing::debug;
use tracing_test::traced_test;

use fs4me_client::Fs;
use fs4me_interface::{Driver, Stat};
use fs4me_local::LocalDriver;

/// Проверяет, что можно получить информацию о драйвере.
#[test]
#[traced_test]
fn test_driver_info() {
    let fs: Fs<LocalDriver> = LocalDriver::connect("").unwrap().into();
    let info_string = fs.driver_info();
    assert!(
        info_string.contains("fs4me-local"),
        "информация о драйвере должна содержать 'fs4me-local'"
    );
}

/// Проверяет, что можно получить текущее время сервера.
#[test]
#[traced_test]
fn test_time() {
    let fs: Fs<LocalDriver> = LocalDriver::connect("").unwrap().into();
    let time = fs.time().unwrap();
    assert!(time > 0, "время сервера должно быть больше 0");

    let now = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as u32;
    assert!(
        time <= now,
        "время сервера должно быть меньше или равно текущему времени, так как они оба представляют собой время одного и того же компьютера"
    );
}

/// Проверяет, что можно проверить существование файла или директории.
#[test]
#[traced_test]
fn test_exists() {
    let fs: Fs<LocalDriver> = LocalDriver::connect("").unwrap().into();
    assert!(fs.exists("."), "файл или директория не должно существовать");

    let rand_string = rand::rng()
        .sample_iter(Alphanumeric)
        .take(10)
        .map(char::from)
        .collect::<String>();

    assert!(
        !fs.exists(&rand_string),
        "файл или директория не должно существовать {rand_string}"
    );
}

/// Тестирование получения списка файлов и директорий
#[test]
#[traced_test]
fn test_stat_ls() {
    let fs: Fs<_> = LocalDriver::connect("").unwrap().into();
    let entries = fs.ls(".").unwrap();
    let dir = entries
        .filter_map(|path| fs.stat(&path).ok().map(|stat| (path, stat)))
        .find(|(_, stat)| matches!(stat, Stat::Dir { .. }));

    debug!(?dir);
    assert!(dir.is_some(), "Хоть одна директория должна быть в списке");
}

/// Тестирование создания директорий
#[test]
#[traced_test]
fn test_mkdir() {
    let fs: Fs<LocalDriver> = LocalDriver::connect("").unwrap().into();
    let root = tempfile::tempdir().unwrap();
    let root_path = root.path();

    // тестирование создания директорий без рекурсии
    for name in 0..3 {
        let dir_path = root_path.join(name.to_string());
        fs.mkdir(&dir_path, false).unwrap();

        assert!(
            fs.exists(&dir_path),
            "директория {name} должна быть создана"
        );
    }

    // тестирование рекурсивного создания директорий
    let a = root_path.join("a");
    let b = a.join("b");
    let c = b.join("c");

    assert!(!fs.exists(&c), "директория c не должна существовать");

    assert!(
        fs.mkdir(&c, false).is_err(),
        "Не должна создаваться {c:?} так как нет родительской директории {b:?}",
    );

    fs.mkdir(&c, true).unwrap();

    assert!(fs.exists(&c), "директория c должна быть создана");
}

/// тестирование перемещения/переименования файлов и директорий
#[test]
#[traced_test]
fn test_mv() {
    let fs: Fs<LocalDriver> = LocalDriver::connect("").unwrap().into();
    let root = tempfile::tempdir().unwrap();

    let root_path = root.path();
    debug!(?root_path);

    let src = root_path.join("src");
    debug!(?src);

    let dst = root_path.join("dst");
    debug!(?dst);

    debug!(?src, "Создание директории");
    fs.mkdir(&src, true).unwrap();

    debug!(?src, "Проверка на существование директории");
    assert!(fs.exists(&src), "Директория src должна быть создана");

    debug!("Перемещение src->dst");
    fs.mv(&src, &dst).unwrap();

    assert!(fs.exists(&dst), "директория dst должна быть создана");
    assert!(!fs.exists(&src), "директория src не должна существовать");
}
