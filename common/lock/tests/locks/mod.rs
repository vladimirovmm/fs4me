use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use fs4me_interface::Driver;
use fs4me_local::LocalDriver;
use fs4me_lock::base_lock::LockPaths;
use fs4me_uuid::FsUuid;
use tempfile::TempDir;
use tracing::info;

mod base_lock_tests;
mod multi_lock_tests;

pub fn read_lock(src: &Path) -> (String, usize) {
    let lock_path = LockPaths::try_from(src).unwrap().path;
    let lock_content = fs::read_to_string(&lock_path).unwrap();
    let lock_count_in_file = lock_content.lines().count();

    (lock_content, lock_count_in_file)
}

pub struct Init {
    pub driver: Arc<LocalDriver>,
    pub uuid: FsUuid,
    pub tmp: TempDir,
    pub source_path: PathBuf,
}

impl Default for Init {
    fn default() -> Self {
        let driver = Arc::new(LocalDriver::connect("").unwrap());
        let uuid = FsUuid::default();

        let tmp = TempDir::with_prefix("test_lock_").unwrap();

        let root_path = tmp.path().to_path_buf();
        info!(?root_path);

        let src = root_path.join("src");
        info!(?src, "Директория для блокировки");

        driver.mkdir(&src, false).unwrap();

        Self {
            driver,
            uuid,
            tmp,
            source_path: src,
        }
    }
}
