use tracing::info;
use tracing_test::traced_test;

use fs4me_client::{Fs, lock::path_to_lock_file};
use fs4me_interface::{Driver, Stat, WriteMode};
use fs4me_local::LocalDriver;

/// Тестирование на чтение и запись в файл.
#[test]
#[traced_test]
fn tests_rw() {
    let fs: Fs<LocalDriver> = LocalDriver::connect(" ").unwrap().into();
    let root = tempfile::tempdir().unwrap();
    // let root_path = root.path();
    let root_path = root.keep();

    // Создание файла для записи
    let file_path = root_path.join("test.txt");

    // Проверка что файл не существует
    assert!(
        !fs.exists(&file_path),
        "Файл не должен существовать до записи {file_path:?}"
    );

    let test_data = b"Hello, Fs4me!";
    let lock_file = path_to_lock_file(&file_path).unwrap();

    // Запись данных в файл
    {
        info!("Открываем файл для записи");
        let mut file = fs.write(&file_path, WriteMode::FailIfExists).unwrap();
        assert!(
            fs.exists(&lock_file),
            "Пока работа с файлом не завершена, должен существовать файл блокировки {lock_file:?}"
        );
        file.write_all(test_data).unwrap();

        info!("Завершаем запись");
        file.flush().unwrap();
    }
    // Блокировка должна быть снята
    assert!(
        !fs.exists(&lock_file),
        "Блокировка должна быть снята {lock_file:?}"
    );

    // Проверка что файл существует после записи
    assert!(
        fs.exists(&file_path),
        "Файл должен существовать после записи"
    );

    // Проверка информации о файле через stat
    let stat = fs.stat(&file_path).unwrap();
    assert!(
        matches!(stat, Stat::File { .. }),
        "Объект должен быть файлом"
    );

    // Чтение данных из файла
    {
        let mut file = fs.read(&file_path, 0).unwrap();
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).unwrap();
        assert_eq!(
            buffer, test_data,
            "Чтение данных должно совпадать с записанными"
        );
    }

    // Проверка корректности lock-файлов
    {
        assert!(!fs.exists(&lock_file), "Lock-файл не должен существовать");
        let parent = lock_file.parent().unwrap();
        assert!(
            !fs.ls(parent)
                .unwrap()
                .any(|p| p.display().to_string().contains("lock")),
            "Lock файлы не должны существовать в родительской директории"
        );
    }
}

/// Тестирование различных режимов записи
///
/// Доступные режимы записи:
/// - `Overwrite`: полное перезаписывание содержимого файла. Если файл не существует, будет создан новый.
/// - `Append`: добавление содержимого в конец файла. Если файл не существует, будет создан новый.
/// - `FailIfExists`: если файл существует, запись не будет выполнена и вернётся ошибка.
#[test]
#[traced_test]
fn test_write_modes() {
    use fs4me_interface::WriteMode;
    let fs: Fs<LocalDriver> = LocalDriver::connect(" ").unwrap().into();
    let root = tempfile::tempdir().unwrap();
    let root_path = root.path();

    // Режим Overwrite - замена содержимого файла
    {
        let file_path = root_path.join("overwrite.txt");
        let mut file = fs.write(&file_path, WriteMode::Overwrite).unwrap();
        file.write_all(b"First write").unwrap();
        file.flush().unwrap();

        // Перезапись
        let mut file = fs.write(&file_path, WriteMode::Overwrite).unwrap();
        file.write_all(b"Overwritten").unwrap();
        file.flush().unwrap();

        // Чтение
        let mut file = fs.read(&file_path, 0).unwrap();
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).unwrap();
        assert_eq!(buffer, b"Overwritten");
    }

    // Режим Append - добавление содержимого в конец файла
    {
        let file_path = root_path.join("append.txt");

        // Сначала создаём файл
        let mut file = fs.write(&file_path, WriteMode::Overwrite).unwrap();
        file.write_all(b"Hello, ").unwrap();
        file.flush().unwrap();

        // Добавляем в конец
        let mut file = fs.write(&file_path, WriteMode::Append).unwrap();
        file.write_all(b"World!").unwrap();
        file.flush().unwrap();

        // Чтение
        let mut file = fs.read(&file_path, 0).unwrap();
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).unwrap();
        assert_eq!(buffer, b"Hello, World!");
    }

    // Режим fail_if_exists - ошибка при попытке записи в существующий файл
    {
        let file_path = root_path.join("fail_if_exists.txt");

        // Создаём файл
        let mut file = fs.write(&file_path, WriteMode::FailIfExists).unwrap();
        file.write_all(b"Hello, ").unwrap();
        file.flush().unwrap();

        // Попытка записи в существующий файл с режимом fail_if_exists должна ошибиться
        let result = fs.write(&file_path, WriteMode::FailIfExists);
        assert!(
            result.is_err(),
            "Запись в существующий файл с режимом fail_if_exists должна ошибиться"
        );

        // Запись в существующий файл с режимом Overwrite должна работать
        let mut file = fs.write(&file_path, WriteMode::Overwrite).unwrap();
        file.write_all(b"Overwritten!").unwrap();
        file.flush().unwrap();

        // Чтение
        let mut file = fs.read(&file_path, 0).unwrap();
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).unwrap();
        assert_eq!(buffer, b"Overwritten!");
    }
}
