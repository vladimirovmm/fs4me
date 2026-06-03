use fs4me_interface::Driver;
use fs4me_lock::base_lock::BaseLock;
use tracing::info;
use tracing_test::traced_test;

use crate::locks::Init;

/// Тест на блокировку при параллельном чтении
#[test]
#[traced_test]
fn test_lock() {
    let Init {
        driver,
        uuid,
        tmp: _tmp,
        source_path,
    } = Default::default();

    let base_lock = BaseLock::try_form(uuid, driver.clone(), source_path).unwrap();
    let content = "test".to_string();
    {
        let _lock = base_lock.try_lock().unwrap();
        info!("Тестирование параллельной блокировки");

        assert!(base_lock.try_lock().is_err(), "Файл уже заблокирован");

        info!("Проверка путей во время блокировки");
        assert!(
            !driver.exists(&base_lock.path),
            "Файл должен быть перемещён в {:?}",
            base_lock.block_path
        );
        assert!(!driver.exists(&base_lock.tmp_path));
        assert!(
            driver.exists(&base_lock.block_path),
            "Файл должен быть перемещён в {:?}",
            base_lock.block_path
        );

        let mut writer = driver
            .write(
                &base_lock.tmp_path,
                fs4me_interface::WriteMode::FailIfExists,
            )
            .unwrap();
        write!(&mut writer, "{content}").unwrap();
        drop(writer);

        assert!(driver.exists(&base_lock.tmp_path));
    }

    info!("Проверка путей после разблокировки");
    assert!(
        !driver.exists(&base_lock.block_path),
        "Файл должен быть перемещён в {:?}",
        base_lock.block_path
    );
    assert!(
        driver.exists(&base_lock.path),
        "Файл должен быть перемещён в {:?}",
        base_lock.path
    );

    info!("Проверка перемещения tmp_path->path");
    assert!(
        !driver.exists(&base_lock.tmp_path),
        "Файл должен быть перемещён в {:?}",
        base_lock.tmp_path
    );

    let mut reader = driver.read(&base_lock.path, 0).unwrap();
    let mut buf = String::new();
    reader.read_to_string(&mut buf).unwrap();
    drop(reader);
    assert_eq!(content, buf);

    info!("Повторная блокировка");
    base_lock.try_lock().unwrap();
}
