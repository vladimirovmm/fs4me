use std::path::PathBuf;

use base64::{Engine, prelude::BASE64_STANDARD};
use fs4me_interface::{Driver, DriverError};
use fs4me_sftp::SftpDriver;
use fs4me_test_infra::{SSH_KEY_PRIVATE, SSH_USER, up_ssh};
use tracing::info;
use tracing_test::traced_test;

fn params_with_key(port: u16) -> String {
    format!(
        "host=localhost\n\
        port={port}\n\
        username={SSH_USER}\n\
        private_key=\"{key}\"",
        key = BASE64_STANDARD.encode(SSH_KEY_PRIVATE)
    )
}

/// Тестирование переименования/перемещения директорий с вложенными директориями.
#[tokio::test]
#[traced_test]
#[cfg_attr(not(feature = "test_with_docker"), ignore)]
async fn test_rename() {
    let ssh_server = up_ssh().await.unwrap();
    let driver = SftpDriver::connect(params_with_key(ssh_server.port)).unwrap();

    let home = PathBuf::from(format!("/home/{SSH_USER}"));
    let root = home.join("tmp");
    driver.mkdir(&root, false).unwrap();
    assert!(
        driver.exists(&root),
        "Директория {root:?} должна существовать"
    );
    info!("корневая директория: {root:?}");

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
#[tokio::test]
#[traced_test]
#[cfg_attr(not(feature = "test_with_docker"), ignore)]
async fn test_rename_nonexistent() {
    let ssh_server = up_ssh().await.unwrap();
    let driver = SftpDriver::connect(params_with_key(ssh_server.port)).unwrap();

    let home = PathBuf::from(format!("/home/{SSH_USER}"));
    let root = home.join("tmp");
    driver.mkdir(&root, false).unwrap();
    assert!(
        driver.exists(&root),
        "Директория {root:?} должна существовать"
    );
    info!("корневая директория: {root:?}");

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
#[tokio::test]
#[traced_test]
#[cfg_attr(not(feature = "test_with_docker"), ignore)]
async fn test_rename_with_files() {
    let ssh_server = up_ssh().await.unwrap();
    let driver = SftpDriver::connect(params_with_key(ssh_server.port)).unwrap();

    let home = PathBuf::from(format!("/home/{SSH_USER}"));
    let root = home.join("tmp");
    driver.mkdir(&root, false).unwrap();
    assert!(
        driver.exists(&root),
        "Директория {root:?} должна существовать"
    );
    info!("корневая директория: {root:?}");

    let a = root.join("a");
    let a1 = a.join("a1");
    driver.mkdir(&a1, true).unwrap();
    assert!(driver.exists(&a1), "{a1:?} должна существовать");

    let b = root.join("b");
    // driver.mkdir(&b, false).unwrap(); // в SFTP нельзя переименовать в существующее имя

    driver.rename(&a, &b).unwrap();
    let new_a1 = b.join("a1");
    assert!(driver.exists(&new_a1), "{new_a1:?} должна существовать");
}
