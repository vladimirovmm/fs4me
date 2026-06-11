use std::path::PathBuf;

use base64::{Engine, prelude::BASE64_STANDARD};
use fs4me_interface::Driver;
use fs4me_sftp::SftpDriver;
use fs4me_test_infra::{SSH_KEY_PRIVATE, SSH_USER, SshServer, up_ssh};
use tracing::info;

pub fn params_with_key(port: u16) -> String {
    format!(
        "host=localhost\n\
        port={port}\n\
        username={SSH_USER}\n\
        private_key=\"{key}\"",
        key = BASE64_STANDARD.encode(SSH_KEY_PRIVATE)
    )
}

pub async fn connect() -> (SshServer, SftpDriver, PathBuf) {
    let ssh_server = up_ssh().await.unwrap();
    let driver = SftpDriver::connect(params_with_key(ssh_server.port)).unwrap();

    let root = PathBuf::from(format!("/home/{SSH_USER}/tmp"));
    driver.mkdir(&root, false).unwrap();
    assert!(
        driver.exists(&root),
        "Директория {root:?} должна существовать"
    );
    info!("корневая директория: {root:?}");

    (ssh_server, driver, root)
}
