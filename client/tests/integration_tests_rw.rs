use std::{
    fs, io,
    path::Path,
    thread::{self, sleep},
    time::Duration,
};

use tracing::{debug, error, info};
use tracing_test::traced_test;

use fs4me_client::Fs;
use fs4me_interface::{Driver, Stat, WriteMode};
use fs4me_local::LocalDriver;
use fs4me_lock::base_lock::LockPaths;

fn err_to_string<S: ToString>(err: S) -> String {
    err.to_string()
}

/// Тестирование на чтение и запись в файл.
#[test]
#[traced_test]
fn tests_rw() {
    let fs: Fs<LocalDriver> = LocalDriver::connect(" ").unwrap().into();
    let root = tempfile::tempdir().unwrap();
    let root_path = root.path();

    // Создание файла для записи
    let file_path = root_path.join("test.txt");

    // Проверка что файл не существует
    assert!(
        !fs.exists(&file_path),
        "Файл не должен существовать до записи {file_path:?}"
    );

    let test_data = b"Hello, Fs4me!";
    let lock_file = <&Path as TryInto<LockPaths>>::try_into(&file_path)
        .unwrap()
        .path;

    // Запись данных в файл
    {
        info!("Открываем файл для записи");
        let mut file = fs.write(&file_path, WriteMode::FailIfExists).unwrap();
        file.write_all(test_data).unwrap();

        info!("Завершаем запись");
        file.flush().unwrap();
        assert!(
            fs.exists(&lock_file),
            "Пока работа с файлом не завершена, должен существовать файл блокировки {lock_file:?}"
        );
    }

    // Проверка что файл существует после записи
    assert!(
        fs.exists(&file_path),
        "Файл должен существовать после записи"
    );
    // Блокировка должна быть снята
    assert!(
        !fs.exists(&lock_file),
        "Блокировка должна быть снята {lock_file:?}"
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
        assert!(
            fs.exists(&lock_file),
            "Блокировка должна существовать {lock_file:?}"
        )
    }

    // Проверка корректности lock-файлов
    assert!(!fs.exists(&lock_file), "Lock-файл не должен существовать");
    let parent = &lock_file.parent().unwrap();
    assert!(
        !fs.ls(parent)
            .unwrap()
            .any(|p| p.display().to_string().contains("lock")),
        "Временных lock-файлов не должно существовать в родительской директории"
    );
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
        {
            let mut file = fs.write(&file_path, WriteMode::Overwrite).unwrap();
            file.write_all(b"First write").unwrap();
            file.flush().unwrap();
        }

        // Перезапись
        {
            let mut file = fs.write(&file_path, WriteMode::Overwrite).unwrap();
            file.write_all(b"Overwritten").unwrap();
            file.flush().unwrap();
        }

        // Чтение
        {
            let mut file = fs.read(&file_path, 0).unwrap();
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer).unwrap();
            assert_eq!(buffer, b"Overwritten");
        }
    }

    // Режим Append - добавление содержимого в конец файла
    {
        let file_path = root_path.join("append.txt");

        // Сначала создаём файл
        {
            let mut file = fs.write(&file_path, WriteMode::Overwrite).unwrap();
            file.write_all(b"Hello, ").unwrap();
            file.flush().unwrap();
        }
        // Добавляем в конец
        {
            let mut file = fs.write(&file_path, WriteMode::Append).unwrap();
            file.write_all(b"World!").unwrap();
            file.flush().unwrap();
        }

        // Чтение
        {
            let mut file = fs.read(&file_path, 0).unwrap();
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer).unwrap();
            assert_eq!(buffer, b"Hello, World!");
        }
    }

    // Режим fail_if_exists - ошибка при попытке записи в существующий файл
    {
        let file_path = root_path.join("fail_if_exists.txt");

        // Создаём файл
        {
            let mut file = fs.write(&file_path, WriteMode::FailIfExists).unwrap();
            file.write_all(b"Hello, ").unwrap();
            file.flush().unwrap();
        }

        // Попытка записи в существующий файл с режимом fail_if_exists должна ошибиться
        let result = fs.write(&file_path, WriteMode::FailIfExists);
        assert!(
            result.is_err(),
            "Запись в существующий файл с режимом fail_if_exists должна ошибиться"
        );

        // Запись в существующий файл с режимом Overwrite должна работать
        {
            let mut file = fs.write(&file_path, WriteMode::Overwrite).unwrap();
            file.write_all(b"Overwritten!").unwrap();
            file.flush().unwrap();
        }

        // Чтение
        {
            let mut file = fs.read(&file_path, 0).unwrap();
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer).unwrap();
            assert_eq!(buffer, b"Overwritten!");
        }
    }
}

/// Тестирование параллельного чтения
#[test]
#[traced_test]
fn test_parallel_read() {
    let fs_client: Fs<LocalDriver> = LocalDriver::connect(" ").unwrap().into();
    let root = tempfile::tempdir().unwrap();
    let root_path = root.path();

    let file_path = root_path.join("test.txt");
    let lock_path = <&Path as TryInto<LockPaths>>::try_into(&file_path)
        .unwrap()
        .path;
    let file_content = "Тестовая запись";

    // Создание файла с тестовым текстом
    {
        let mut writer = fs_client
            .write(&file_path, WriteMode::FailIfExists)
            .unwrap();
        writeln!(&mut writer, "{file_content}").unwrap();
        writer.flush().unwrap();
    }

    // Создание нескольких потоков чтения одного файла
    let count = 10;
    let threads = (0..count)
        .into_iter()
        .map(|thread_num| {
            let fs = fs_client.clone();
            let file_path = file_path.clone();
            let lock_path = lock_path.clone();
            thread::spawn(move || {
                debug!(?thread_num, "Открытие файла для чтения");

                // Создание буфера чтения файла
                let mut reader = fs.read(&file_path, 0).unwrap();

                // Задержка, чтобы другие потоки успели тоже открыть файл для чтения
                sleep(Duration::from_secs(1));

                // чтение lock файла чтобы убедиться что записи блокировок добавляются
                let lock_lines_count = fs::read_to_string(&lock_path)
                    .inspect_err(|err| error!(?err, "Ошибка при чтении lock файла"))
                    .unwrap_or_default()
                    .lines()
                    .count();

                // Чтение полностью файла
                let content = io::read_to_string(&mut reader).unwrap();

                if file_content != content.trim() {
                    Err("Содержимое файла должно быть равно исходному")
                } else {
                    Ok(lock_lines_count)
                }
            })
        })
        .collect::<Vec<_>>();

    // Ожидание завершения всех потоков
    let result = threads
        .into_iter()
        .map(|thread| thread.join())
        .collect::<Result<Vec<_>, _>>();

    match result {
        Ok(results) => {
            let max = results.into_iter().map(|r| r.unwrap()).max().unwrap();
            assert_eq!(count, max);
        }
        Err(err) => {
            panic!("{err:#?}");
        }
    }

    assert!(
        !fs_client.exists(&lock_path),
        "Файл блокировки должен быть удалён, так как все потоки завершили чтение"
    );
}

/// Тест для проверки очереди на запись
#[test]
#[traced_test]
fn test_write_queue() {
    let fs_client: Fs<LocalDriver> = LocalDriver::connect(" ").unwrap().into();
    let root = tempfile::tempdir().unwrap();
    let root_path = root.path();

    let file_path = root_path.join("test.txt");
    let lock_path = <&Path as TryInto<LockPaths>>::try_into(&file_path)
        .unwrap()
        .path;
    let count_threads = 5;
    // Создание нескольких потоков для записи одного файла
    let threads = (0..count_threads)
        .into_iter()
        .map(|thread_num| {
            let fs = fs_client.clone();
            let file_path = file_path.clone();
            let lock_path = lock_path.clone();
            thread::spawn(move || {
                debug!(?thread_num, "Открытие файла для чтения");

                debug!(
                    ?thread_num,
                    "==== Start ======================================="
                );
                // Создание буфера чтения файла
                let mut writer = fs
                    .write(&file_path, WriteMode::Append)
                    .map_err(err_to_string)?;
                // Задержка, чтобы другие потоки успели тоже открыть файл для чтения
                debug!(
                    ?thread_num,
                    "==== sleep start ======================================="
                );
                sleep(Duration::from_secs(1));
                debug!(
                    ?thread_num,
                    "==== sleep end ======================================="
                );

                // чтение lock файла чтобы убедиться что записи блокировок добавляются
                let lock_lines_count = fs::read_to_string(&lock_path)
                    .inspect(|content| debug!(?content))
                    .inspect_err(|err| error!(?err, "Ошибка при чтении lock файла"))
                    .unwrap_or_default()
                    .lines()
                    .count();

                // Чтение полностью файла
                writeln!(&mut writer, "{thread_num}").map_err(err_to_string)?;

                debug!(
                    ?thread_num,
                    "==== end ======================================="
                );
                Ok::<usize, String>(lock_lines_count)
            })
        })
        .collect::<Vec<_>>();

    // Ожидание завершения всех потоков
    let results = threads
        .into_iter()
        .map(|thread| thread.join())
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    let max = results.into_iter().map(|res| res.unwrap()).max().unwrap();
    assert_eq!(count_threads, max);

    assert!(
        !fs_client.exists(&lock_path),
        "Файл блокировки должен быть удалён, так как все потоки завершили запись"
    );
}
