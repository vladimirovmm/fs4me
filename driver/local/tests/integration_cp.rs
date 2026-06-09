use std::fs;

use fs4me_local::LocalDriver;
use tempfile::tempdir;
use tracing_test::traced_test;

use fs4me_interface::{Driver, DriverParams, WriteMode};

/// Тестирование копирования файла через LocalDriver.
///
/// src.txt -> dst.txt
#[test]
#[traced_test]
fn test_driver_copy_file() {
    let root = tempdir().unwrap();
    let root_path = root.path();
    let driver = LocalDriver::connect(DriverParams::default()).unwrap();

    let content = "Hello, World!";
    let file_src = root_path.join("src.txt");
    let mut writer = driver.write(&file_src, WriteMode::FailIfExist).unwrap();
    writer.write_all(content.as_bytes()).unwrap();
    drop(writer);

    let file_dst = root_path.join("dst.txt");

    driver.copy_file(&file_src, &file_dst).unwrap();
    assert!(driver.exists(&file_dst), "Файл должен существовать");
    let content = fs::read_to_string(&file_dst).unwrap();
    assert_eq!(content, content);
}
