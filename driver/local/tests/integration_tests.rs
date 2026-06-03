use fs4me_local::LocalDriver;
use tempfile::tempdir;
use tracing::info;
use tracing_test::traced_test;

use fs4me_interface::{Driver, DriverParams, WriteMode};

#[test]
#[traced_test]
fn test_driver_info() {
    let driver = LocalDriver::connect(DriverParams::default()).unwrap();
    let name = driver.name();
    info!("Name: {name}");
    let version = driver.version();
    info!("Version: {version}");
    assert!(!name.is_empty());
    assert!(!version.is_empty());
}

#[test]
#[traced_test]
fn test_time() {
    let driver = LocalDriver::connect(DriverParams::default()).unwrap();
    let server_time = driver.time().unwrap();
    info!("Server time: {server_time:?}");
    let local_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    info!("Local time: {local_time}");
}

#[test]
#[traced_test]
fn test_ls() {
    let tmp_dir = tempdir().unwrap();
    let root_path = tmp_dir.path();
    let dir_0 = root_path.join("0");

    info!("Временная директория: {root_path:?}");

    let driver = LocalDriver::connect(DriverParams::default()).unwrap();

    let mut iter = driver.ls(root_path).unwrap();
    assert!(iter.next().is_none(), "Директория должна быть пустой");
    assert!(!driver.exists(&dir_0), "Директория не должна существовать");

    // Создание директорий
    for dir_name in 0..10 {
        let dir_path = root_path.join(dir_name.to_string());
        driver.mkdir(dir_path, false).unwrap();
    }

    let files = driver.ls(root_path).unwrap().collect::<Vec<_>>();
    assert_eq!(files.len(), 10, "Должно быть 10 директорий");

    assert!(driver.exists(&dir_0), "Директория должна существовать");

    driver.rm(&dir_0).unwrap();
    assert!(!driver.exists(&dir_0), "Директория должна быть удалена");
}

/// Тестирование работы LocalDriver с директориями.
/// Проверяет создание, удаление, перечисление и проверку существования директорий.
#[test]
#[traced_test]
fn test_work_with_directory() {
    let tmp_dir = tempdir().unwrap();
    info!("Временная директория: {:?}", tmp_dir.path());

    let driver = LocalDriver::connect(DriverParams::default()).unwrap();

    let root = tmp_dir.path();
    let a = root.join("a");
    let a1 = a.join("a1");
    let a2 = a1.join("a2");

    // Проверка начального состояния
    assert!(
        driver.ls(root).unwrap().next().is_none(),
        "Директория должна быть пустой"
    );
    assert!(
        !driver.exists(&a1),
        "Директория ./a/a1 не должна существовать"
    );
    assert!(
        driver.mkdir(&a1, false).is_err(),
        "Нельзя создать ./a/a1, так как ./a не существует"
    );

    // Создание рекурсивной структуры
    driver.mkdir(&a2, true).unwrap();
    assert!(
        driver.exists(&a2),
        "Директория ./a/a1/a2 должна существовать"
    );

    // Создание простых директорий в корне
    for dir_name in ["b", "c", "d"] {
        let path = root.join(dir_name);
        driver.mkdir(&path, false).unwrap();
    }

    assert_eq!(
        driver.ls(root).unwrap().count(),
        4,
        "В корне должно быть 4 директории: a, b, c, d"
    );

    // Перемещение в корзину
    driver
        .rm(&a)
        .expect("Должно быть успешно удалено целое дерево ./a");
    assert!(!driver.exists(&a), "Директория ./a должна быть удалена");

    assert_eq!(
        driver.ls(&root).unwrap().count(),
        3,
        "В корне должно быть 4 директории: b, c, d, .trash"
    );
}

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
