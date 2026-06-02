use rand::{RngExt, distr::Alphanumeric};
use tempfile::TempDir;
use tracing::debug;
use tracing_test::traced_test;

use fs4me_client::{Fs, lock::path_to_lock_file};
use fs4me_interface::{Driver, Stat};
use fs4me_local::LocalDriver;

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
    let root = TempDir::with_prefix("test_mv_").unwrap();

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

    debug!("Проверка на lock-файлы. Они должны быть удалены по завершению операции");
    for path in [&src, &dst] {
        debug!(?path, "ищем lock-файлы в директории");
        let lock_file = path_to_lock_file(path).unwrap();
        assert!(
            !fs.exists(&lock_file),
            "lock-файл не должен существовать {lock_file:?}"
        );
        let parent = path.parent().unwrap();
        assert!(
            !fs.ls(parent)
                .unwrap()
                .inspect(|path| debug!(?path))
                .any(|path| path.display().to_string().contains("lock")),
            "Lock файл найден"
        );
    }
}
