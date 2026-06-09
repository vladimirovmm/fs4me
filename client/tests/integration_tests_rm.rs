use std::fs;

use tempfile::TempDir;
use tracing::debug;
use tracing_test::traced_test;

use fs4me_client::Fs;
use fs4me_interface::Driver;
use fs4me_local::LocalDriver;

/// тестирование удаления файлов и директорий
#[test]
#[traced_test]
fn test_rm() {
    let root = TempDir::with_prefix("test_rm_").unwrap();
    let fs: Fs<LocalDriver> = LocalDriver::connect("").unwrap().into();

    let root_path = root.path();
    debug!(?root_path);

    for path_str in ["a", "a/b", "a/b/c"] {
        let dir_path = root_path.join(path_str);
        fs::create_dir(&dir_path).unwrap();
        for file_name in 0..3 {
            let file_path = dir_path.join(file_name.to_string()).with_extension(".txt");
            fs::write(&file_path, "demo").unwrap();
            fs.rm(&file_path).unwrap();
        }
    }

    assert!(
        root_path.join("a/b/.trash/0.txt").exists(),
        "файл должен быть перемещен в корзину"
    );
    fs.clear_trash(&root_path).unwrap();
    assert!(
        !root_path.join("a/b/.trash/").exists(),
        "Корзина должна быть удалена"
    );
}
