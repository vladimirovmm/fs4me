use base64::{Engine, prelude::BASE64_STANDARD};
use fs4me_interface::Driver;
use fs4me_sftp::SftpDriver;
use fs4me_test_infra::{SSH_KEY_PRIVATE, SSH_PASSWORD, SSH_USER, up_ssh};
use tracing_test::traced_test;

/// Авторизация по паролю
#[tokio::test]
#[traced_test]
#[cfg_attr(not(feature = "test_with_docker"), ignore)]
async fn test_base_connect_by_password() {
    let ssh_server = up_ssh().await.unwrap();
    // sleep(Duration::from_secs(2));
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
    // sleep(Duration::from_secs(2));
    let _driver = SftpDriver::connect(format!(
        "host=localhost\n\
        port={}\n\
        username={SSH_USER}\n\
        private_key=\"{key}\"",
        ssh_server.port,
        key = BASE64_STANDARD.encode(SSH_KEY_PRIVATE)
    ))
    .unwrap();
}
