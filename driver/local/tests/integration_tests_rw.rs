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
        let mut fopen = driver.write(&file_path, WriteMode::FailIfExist).unwrap();
        assert!(
            driver.exists(&file_path),
            "Файл {file_path:?} должен существовать после его открытия"
        );
        writeln!(&mut fopen, "a").unwrap();
        writeln!(&mut fopen, "b").unwrap();
        drop(fopen);
    }
    // Тестирование чтения
    {
        let mut fopen = driver.read(&file_path, 0).unwrap();
        let mut buf = String::new();
        fopen.read_to_string(&mut buf).unwrap();
        assert_eq!(buf, "a\nb\n");
    }

    // попытка записи в существующий файл с флагом FailIfExist запрещающий запись в существующий файл
    {
        assert!(
            driver.write(&file_path, WriteMode::FailIfExist).is_err(),
            "Должно быть ошибка при записи в существующий файл"
        );
    }

    // дозапись
    {
        let mut fopen = driver.write(&file_path, WriteMode::Append).unwrap();
        writeln!(&mut fopen, "c").unwrap();
        drop(fopen);
    }
    // тестирование чтения с указанием позиции
    {
        let mut fopen = driver.read(&file_path, 4).unwrap();
        let mut buf = String::new();
        fopen.read_to_string(&mut buf).unwrap();
        assert_eq!(buf, "c\n");
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

/// Попытка одновременного чтения и записи
///
/// Процесс:
/// 1. Создаём файл `a.txt`
/// 2. Открываем его для записи
/// 3. Пишем данные в `a.txt`
/// 4. Открываем для чтения
/// 5. Читаем содержимое
/// 6. Закрываем чтение и запись
#[test]
#[traced_test]
fn test_concurrent_read_write() {
    let tmp_dir = tempdir().unwrap();
    info!("Временная директория: {:?}", tmp_dir.path());

    let driver = LocalDriver::connect(DriverParams::default()).unwrap();

    let root = tmp_dir.path();
    let file_path = root.join("a.txt");
    info!("Создание файла {file_path:?}. Только если его нет");

    // Отырваем файл для записи и записи
    let mut writer = driver.write(&file_path, WriteMode::FailIfExist).unwrap();
    writeln!(&mut writer, "hello world").unwrap();
    writer.flush().unwrap(); // Без этого файл будет пустой

    // Затем открываем файл для чтения
    let mut reader = driver.read(&file_path, 0).unwrap();
    let mut content = String::new();
    reader.read_to_string(&mut content).unwrap();

    // Проверяем, что данные совпадают
    assert_eq!(content, "hello world\n");
}

/// Попытка одновременной записи c перезаписью
///
/// Процесс:
/// 1. Создаём файл `a.txt`
/// 2. Открываем его для записи writer_1 + Overwrite
/// 3. Открываем его для записи writer_2 + Overwrite
/// 4. writer_1 пишет данные + flush
/// 5. writer_2 пишет данные + flush
/// 6. Читаем содержимое и сверяем
#[test]
#[traced_test]
fn test_concurrent_write_overwrite() {
    let tmp_dir = tempdir().unwrap();
    info!("Временная директория: {:?}", tmp_dir.path());

    let driver = LocalDriver::connect(DriverParams::default()).unwrap();

    let root = tmp_dir.path();
    let file_path = root.join("a.txt");
    info!("Создание файла {file_path:?}. Только если его нет");

    // Используем Overwrite, чтобы второй writer перезаписал файл
    let mut writer_1 = driver.write(&file_path, WriteMode::Overwrite).unwrap();
    let mut writer_2 = driver.write(&file_path, WriteMode::Overwrite).unwrap();

    // Первый writer пишет
    writeln!(&mut writer_1, "writer_1 data").unwrap();
    writer_1.flush().unwrap();

    // Второй writer перезаписывает и пишет
    write!(&mut writer_2, "writer_2 data").unwrap();
    writer_2.flush().unwrap();

    drop(writer_1);
    drop(writer_2);

    // Читаем и проверяем, что файл содержит данные от writer_2
    let mut content = String::new();
    {
        let mut reader = driver.read(&file_path, 0).unwrap();
        reader.read_to_string(&mut content).unwrap();
    }

    assert_eq!(content.trim(), "writer_2 data");
}

/// Попытка одновременной записи с добавлением
///
/// Процесс:
/// 1. Создаём файл `a.txt`
/// 2. Открываем его для записи writer_1 + Append
/// 3. Открываем его для записи writer_2 + Append
/// 4. writer_1 пишет данные + flush
/// 5. writer_2 пишет данные + flush
/// 6. Читаем содержимое и сверяем
#[test]
#[traced_test]
fn test_concurrent_write_append() {
    let tmp_dir = tempdir().unwrap();
    info!("Временная директория: {:?}", tmp_dir.path());

    let driver = LocalDriver::connect(DriverParams::default()).unwrap();

    let root = tmp_dir.path();
    let file_path = root.join("a.txt");
    info!("Создание файла {file_path:?}. Только если его нет");

    // Используем Append, чтобы второй writer добавил данные в конец файла
    let mut writer_1 = driver.write(&file_path, WriteMode::Append).unwrap();
    let mut writer_2 = driver.write(&file_path, WriteMode::Append).unwrap();

    // Первый writer пишет
    writeln!(&mut writer_1, "writer_1 data").unwrap();
    writer_1.flush().unwrap();

    // Второй writer добавляет данные в конец файла
    write!(&mut writer_2, "writer_2 data").unwrap();
    writer_2.flush().unwrap();

    drop(writer_1);
    drop(writer_2);

    // Читаем и проверяем, что файл содержит данные от writer_2
    let mut content = String::new();
    {
        let mut reader = driver.read(&file_path, 0).unwrap();
        reader.read_to_string(&mut content).unwrap();
    }

    assert_eq!(content.trim(), "writer_1 data\nwriter_2 data");
}

/// Тестирование записи во время перемещения файла
///
/// Процесс:
/// 1. Создаём файл `a.txt`
/// 2. Открываем его для записи
/// 3. Перемещаем его в `b.txt`
/// 4. Продолжаем записывать в `a.txt`
/// 5. Закрываем файл
/// 6. Проверяем, что файл `b.txt` содержит данные
#[test]
#[traced_test]
fn test_read_write_during_rename_file() {
    let tmp_dir = tempdir().unwrap();
    info!("Временная директория: {:?}", tmp_dir.path());

    let driver = LocalDriver::connect(DriverParams::default()).unwrap();

    let root = tmp_dir.path();
    let file_a = root.join("a.txt");
    info!("Создание файла {file_a:?}. Только если его нет");

    // Открытие для записи только если файл не существует

    let mut fopen = driver.write(&file_a, WriteMode::FailIfExist).unwrap();
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

/// Тестирование записи во время перемещения файла родительской директории
///
/// Процесс:
/// 1. Создаём файл `src/a.txt`
/// 2. Открываем его для записи
/// 3. Перемещаем его в `dst/b.txt`
/// 4. Продолжаем записывать в `src/a.txt`
/// 5. Закрываем файл
/// 6. Проверяем, что файл `dst/b.txt` содержит данные
#[test]
#[traced_test]
fn test_write_during_rename_parent() {
    let root = tempfile::tempdir().unwrap();
    let root_path = root.path();

    let src = root_path.join("src");
    fs::create_dir_all(&src).unwrap();
    let file_src = src.join("a.txt");

    let dst = root_path.join("dst");
    let file_dst = dst.join("a.txt");

    let driver = LocalDriver::connect(DriverParams::default()).unwrap();

    let mut writer = driver.write(&file_src, WriteMode::FailIfExist).unwrap();
    write!(&mut writer, "1").unwrap();
    writer.flush().unwrap();

    driver.rename(&src, &dst).unwrap();
    write!(&mut writer, "2").unwrap();
    writer.flush().unwrap();
    drop(writer);

    let content = fs::read_to_string(&file_dst).unwrap();
    info!(?content, "Содержимое файла");
    assert_eq!(content, "12");
}

/// Тестирование чтения во время перемещения файла
///
/// Процесс:
/// 1. Создаём файл `a.txt`
/// 2. Открываем его для чтения
/// 3. Перемещаем его в `b.txt`
/// 4. Продолжаем читать
/// 5. Проверяем, что файл `b.txt` содержит данные
#[test]
#[traced_test]
fn test_read_during_rename_file() {
    let tmp_dir = tempdir().unwrap();
    info!("Временная директория: {:?}", tmp_dir.path());

    let driver = LocalDriver::connect(DriverParams::default()).unwrap();

    let root = tmp_dir.path();
    let file_a = root.join("a.txt");
    info!("Создание файла {file_a:?}. Только если его нет");

    // Открытие для записи только если файл не существует
    let mut writer = driver.write(&file_a, WriteMode::FailIfExist).unwrap();
    assert!(
        driver.exists(&file_a),
        "Файл {file_a:?} должен существовать после его открытия"
    );
    write!(&mut writer, "1").unwrap();
    drop(writer);

    let mut reader = driver.read(&file_a, 0).unwrap();

    let file_b = file_a.with_extension("b.txt");
    driver.rename(&file_a, &file_b).unwrap();

    let mut content = String::new();
    reader.read_to_string(&mut content).unwrap();
    assert_eq!(content, "1");
}
