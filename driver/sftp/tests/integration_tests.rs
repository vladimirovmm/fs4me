use std::path::PathBuf;

use base64::{Engine, prelude::BASE64_STANDARD};
use fs4me_interface::Driver;
use fs4me_sftp::SftpDriver;
use fs4me_test_infra::{SSH_KEY_PRIVATE, SSH_PASSWORD, SSH_USER, up_ssh};
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

/// Авторизация по паролю
#[tokio::test]
#[traced_test]
#[cfg_attr(not(feature = "test_with_docker"), ignore)]
async fn test_base_connect_by_password() {
    let ssh_server = up_ssh().await.unwrap();
    let _driver = SftpDriver::connect(format!(
        "host=localhost\n\
        port={}\n\
        username={SSH_USER}\n\
        password={SSH_PASSWORD}",
        ssh_server.port,
    ))
    .unwrap();
}

/// Авторизация по ключу
#[tokio::test]
#[traced_test]
#[cfg_attr(not(feature = "test_with_docker"), ignore)]
async fn test_base_connect_by_key() {
    let ssh_server = up_ssh().await.unwrap();
    let _driver = SftpDriver::connect(params_with_key(ssh_server.port)).unwrap();
}

#[tokio::test]
#[traced_test]
#[cfg_attr(not(feature = "test_with_docker"), ignore)]
async fn test_driver_info() {
    let ssh_server = up_ssh().await.unwrap();
    let driver = SftpDriver::connect(params_with_key(ssh_server.port)).unwrap();

    let name = driver.name();
    info!("Name: {name}");
    let version = driver.version();
    info!("Version: {version}");
    assert!(!name.is_empty());
    assert!(!version.is_empty());
}

#[tokio::test]
#[traced_test]
#[cfg_attr(not(feature = "test_with_docker"), ignore)]
async fn test_time() {
    let ssh_server = up_ssh().await.unwrap();
    let driver = SftpDriver::connect(params_with_key(ssh_server.port)).unwrap();

    let server_time = driver.server_time().unwrap();
    info!("Server time: {server_time:?}");
    let local_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap();
    info!("Local time: {local_time:?}");
    assert!(local_time.as_secs() - server_time.as_secs() <= 1); // 1 секунда погрешности на стыке времени
}

/// Тестирование работы с директориями.
/// Проверяет создание, удаление, перечисление и проверку существования директорий.
#[tokio::test]
#[traced_test]
#[cfg_attr(not(feature = "test_with_docker"), ignore)]
async fn test_work_with_directory() {
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
    let a2 = a1.join("a2");

    // Проверка начального состояния
    assert!(
        driver.ls(&root).unwrap().next().is_none(),
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
        driver.ls(&root).unwrap().count(),
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
        "В корне должно быть 4 директории: b, c, d"
    );
}
