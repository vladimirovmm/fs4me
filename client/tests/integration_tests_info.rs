use std::time::SystemTime;
use tracing_test::traced_test;

use fs4me_client::Fs;
use fs4me_interface::Driver;
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
