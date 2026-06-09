//! Тут есть тесты с проверкой элементарных базовых вещей, которые как бы даже и проверять не нужно.
//! Но из-за того, что от этих принципов зависит корректная работа блокировки (Lock), они, как гарантия её правильной работы, реализованы.

use fs4me_local::LocalDriver;
use tempfile::tempdir;
use tracing::info;
use tracing_test::traced_test;

use fs4me_interface::{Driver, DriverError, DriverParams};

/// Тестирование переименования/перемещения директорий с вложенными директориями.
#[test]
#[traced_test]
fn test_rename() {
    let tmp_dir = tempdir().unwrap();
    info!("Временная директория: {:?}", tmp_dir.path());

    let driver = LocalDriver::connect(DriverParams::default()).unwrap();

    let root = tmp_dir.path();
    let a = root.join("a");
    let a1 = a.join("a1");
    let b = root.join("b");

    driver.mkdir(&a1, true).unwrap();
    assert!(driver.exists(&a), "Директория {a:?} должна существовать");
    assert!(
        !driver.exists(&b),
        "Директория {b:?} не должна существовать"
    );
    driver.rename(&a, &b).unwrap();
    assert!(
        !driver.exists(&a),
        "Директория {a:?} не должна существовать после переименования"
    );
    assert!(
        driver.exists(&b),
        "Директория {b:?} должна существовать после переименования"
    );
    assert!(
        driver.exists(b.join("a1")),
        "Директория `b/a1` должна существовать"
    );
}

/// Тестирование переименования/перемещения несуществующей директории
///
#[test]
#[traced_test]
fn test_rename_nonexistent() {
    let tmp_dir = tempdir().unwrap();
    info!("Временная директория: {:?}", tmp_dir.path());

    let driver = LocalDriver::connect(DriverParams::default()).unwrap();

    let root = tmp_dir.path();
    let from = root.join("from");
    assert!(!driver.exists(&from), "{from:?} не должно существовать");
    let to = root.join("to");
    assert!(!driver.exists(&to), "{to:?} не должно существовать");

    assert!(
        matches!(
            driver.rename(&from, &to).err().unwrap(),
            DriverError::PathExistsError(_),
        ),
        "Ошибка должна быть PathExistsError"
    );

    driver.mkdir(&from, false).unwrap();
    driver.rename(&from, &to).unwrap();
    assert!(driver.exists(&to), "{to:?} должно существовать");
    assert!(!driver.exists(&from), "{from:?} не должно существовать");

    assert!(
        matches!(
            driver.rename(&from, &to).err().unwrap(),
            DriverError::PathExistsError(_),
        ),
        "Ошибка должна быть PathExistsError"
    );
}

/// Проверка, что перемещение происходит вместе с вложенными файлами.
#[test]
#[traced_test]
fn test_rename_with_files() {
    let tmp_dir = tempdir().unwrap();
    info!("Временная директория: {:?}", tmp_dir.path());

    let driver = LocalDriver::connect(DriverParams::default()).unwrap();

    let root = tmp_dir.path();
    let a = root.join("a");
    let a1 = a.join("a1");
    driver.mkdir(&a1, true).unwrap();
    assert!(driver.exists(&a1), "{a1:?} должна существовать");

    let b = root.join("b");
    driver.mkdir(&b, false).unwrap();

    driver.rename(&a, &b).unwrap();
    let new_a1 = b.join("a1");
    assert!(driver.exists(&new_a1), "{new_a1:?} должна существовать");
}
