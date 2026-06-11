use fs4me_interface::{Driver, WriteMode};
use tracing_test::traced_test;

use crate::init::connect;

mod init;

/// Тестирование копирования файла через LocalDriver.
///
/// src.txt -> dst.txt
#[tokio::test]
#[traced_test]
#[cfg_attr(not(feature = "test_with_docker"), ignore)]
async fn test_driver_copy_file() {
    let (_ssh_server, driver, root) = connect().await;

    let content = "Hello, World!";
    let file_src = root.join("src.txt");
    {
        let mut writer = driver.write(&file_src, WriteMode::FailIfExist).unwrap();
        writer.write_all(content.as_bytes()).unwrap();
        drop(writer);
    }

    let file_dst = root.join("dst.txt");

    driver.copy_file(&file_src, &file_dst).unwrap();
    assert!(driver.exists(&file_dst), "Файл должен существовать");
    let mut file_content = String::new();
    let mut reader = driver.read(&file_dst, 0).unwrap();
    reader.read_to_string(&mut file_content).unwrap();
    assert_eq!(content, file_content);
}
