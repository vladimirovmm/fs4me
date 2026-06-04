use std::fs;

use fs4me_local::LocalDriver;
use tempfile::tempdir;
use tracing::info;
use tracing_test::traced_test;

use fs4me_interface::{Driver, DriverParams, WriteMode};

/// Тестирование методов записи и чтение результата
#[test]
#[traced_test]
fn test_rw() {
    let tmp_dir = tempdir().unwrap();
    info!("Временная директория: {:?}", tmp_dir.path());

    let driver = LocalDriver::connect(DriverParams::default()).unwrap();

    let root = tmp_dir.path();
    let file_path = root.join("demo.txt");
    info!("Создание файла {file_path:?}. Только если его нет");

    // Открытие для записи только если файл не существует
    {
        let mut fopen = driver.write(&file_path, WriteMode::FailIfExists).unwrap();
        assert!(
            driver.exists(&file_path),
            "Файл {file_path:?} должен существовать после его открытия"
        );
        writeln!(&mut fopen, "a").unwrap();
        drop(fopen);
    }
    // Тестирование чтения
    {
        let mut fopen = driver.read(&file_path, 0).unwrap();
        let mut buf = String::new();
        fopen.read_to_string(&mut buf).unwrap();
        assert_eq!(buf, "a\n");
    }

    // попытка записи в существующий файл с флагом FailIfExists запрещающий запись в существующий файл
    {
        assert!(
            driver.write(&file_path, WriteMode::FailIfExists).is_err(),
            "Должно быть ошибка при записи в существующий файл"
        );
    }

    // дозапись
    {
        let mut fopen = driver.write(&file_path, WriteMode::Append).unwrap();
        writeln!(&mut fopen, "b").unwrap();
        drop(fopen);
    }
    // тестирование чтения с указанием позиции
    {
        let mut fopen = driver.read(&file_path, 2).unwrap();
        let mut buf = String::new();
        fopen.read_to_string(&mut buf).unwrap();
        assert_eq!(buf, "b\n");
    }

    // тестирование перезаписи
    {
        let mut fopen = driver.write(&file_path, WriteMode::Overwrite).unwrap();
        write!(&mut fopen, "c").unwrap();
        drop(fopen);
    }
    // тестирование чтения после перезаписи
    {
        let mut fopen = driver.read(&file_path, 0).unwrap();
        let mut buf = String::new();
        fopen.read_to_string(&mut buf).unwrap();
        assert_eq!(buf, "c");
    }
}

/// Тестирование чтения записи во время перемещения файла
///
/// Для локальной файловой системы возможно продолжение записи даже при перемещении
#[test]
#[traced_test]
fn test_read_write_during_move() {
    let tmp_dir = tempdir().unwrap();
    info!("Временная директория: {:?}", tmp_dir.path());

    let driver = LocalDriver::connect(DriverParams::default()).unwrap();

    let root = tmp_dir.path();
    let file_a = root.join("a.txt");
    info!("Создание файла {file_a:?}. Только если его нет");

    // Открытие для записи только если файл не существует

    let mut fopen = driver.write(&file_a, WriteMode::FailIfExists).unwrap();
    assert!(
        driver.exists(&file_a),
        "Файл {file_a:?} должен существовать после его открытия"
    );
    write!(&mut fopen, "1").unwrap();
    fopen.flush().unwrap();

    let file_b = root.join("b.txt");
    driver.rename(&file_a, &file_b).unwrap();
    write!(&mut fopen, "2").unwrap();

    drop(fopen);

    info!("Содержимое корневой директории");
    for path in driver.ls(&root).unwrap() {
        info!(?path);
    }

    let content = fs::read_to_string(&file_b).unwrap();
    info!(?content, "Содержимое файла");
    assert_eq!(content, "12");
}
