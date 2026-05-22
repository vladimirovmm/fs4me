use std::{
    ffi::{CString, c_void},
    path::PathBuf,
};
use tracing::{debug, info};

use fs4me_interface::ptr_to_string;
use rand::distr::{Alphanumeric, SampleString};
use tracing_test::traced_test;

use crate::{
    client_connect, client_disconnect, client_exists, client_ls, client_ls_free,
    client_ls_has_next, client_ls_next, client_mkdir, client_rmdir, free_c_string, path_to_cstring,
};

/// Временная директория, которая создается автоматически и удаляется при выходе из области видимости.
///
/// Стандартные библиотеки для работы с временными файлами (например, `tempfile`) неприменимы в данном контексте,
/// так как целевая директория может находиться на произвольных файловых системах (локальная, SFTP, FTP, WebDAV и т.д.),
/// а доступ к этим системам осуществляется исключительно через FFI-интерфейс.
struct TempDir {
    path: PathBuf,
    client: *mut c_void,
}

impl TempDir {
    pub fn new(client: *mut c_void) -> Self {
        let root = PathBuf::from(&format!(
            "./temp_{}",
            Alphanumeric.sample_string(&mut rand::rng(), 16)
        ));
        let root_cstring = path_to_cstring(&root);

        // если папка уже существует, удаляем ее. Для тестов нужна пустая папка
        unsafe {
            if client_exists(client, root_cstring.as_ptr()) {
                assert_eq!(
                    client_rmdir(client, root_cstring.as_ptr(), true),
                    0,
                    "Ошибка при удалении временной директории {root:?}"
                );
            }
        }
        // создаем пустую папку
        unsafe {
            assert_eq!(
                client_mkdir(client, root_cstring.as_ptr(), false),
                0,
                "Ошибка при создании временной директории {root:?}"
            );
        }

        Self { path: root, client }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let root = &self.path;
        let root_cstring = path_to_cstring(root);
        unsafe {
            assert_eq!(
                client_rmdir(self.client, root_cstring.as_ptr(), true),
                0,
                "Ошибка при удалении временной директории {root:?}"
            );
        }
    }
}

#[test]
#[traced_test]
fn test_dir() {
    let params_cstring = std::ffi::CString::new("").unwrap();

    // Подключение
    let client = unsafe { client_connect(params_cstring.as_ptr()) };
    assert!(!client.is_null(), "Подключение не удалось");

    let temp_dir = TempDir::new(client);
    let root = &temp_dir.path;
    let root_cstring = path_to_cstring(root);

    let a = root.join("a");
    let a_cstring = path_to_cstring(&a);
    let a1 = a.join("a1");
    let a1_cstring = path_to_cstring(&a1);
    let a2 = a1.join("a2");
    let a2_cstring = path_to_cstring(&a2);

    // === тестирование не рекурсивного создания

    unsafe {
        assert_eq!(
            client_mkdir(client, a2_cstring.as_ptr(), false),
            3,
            "Папка не должна быть создана. Родительская папка не существует"
        );
    }

    unsafe {
        assert!(
            !client_exists(client, a_cstring.as_ptr()),
            "Папка существует"
        );
        assert_eq!(
            client_mkdir(client, a_cstring.as_ptr(), false),
            0,
            "Папка не была создана"
        );
        assert!(
            client_exists(client, a_cstring.as_ptr()),
            "Папка должна существовать"
        );
    }

    // === тестирование рекурсивного создания
    unsafe {
        for dir_cstring in [&a1_cstring, &a2_cstring] {
            assert!(
                !client_exists(client, dir_cstring.as_ptr()),
                "Папка не должна существовать {dir_cstring:?}"
            );
        }

        assert_eq!(
            client_mkdir(client, a2_cstring.as_ptr(), true),
            0,
            "Папка не была создана"
        );

        for dir_cstring in [&a_cstring, &a1_cstring, &a2_cstring] {
            assert!(
                client_exists(client, dir_cstring.as_ptr()),
                "Папка должна существовать {dir_cstring:?}"
            );
        }
    }
    // === тестирование LS команды
    for dir_name in 0..10 {
        let dir_path = root.join(dir_name.to_string());
        let dir_path_cstring = CString::new(dir_path.to_string_lossy().into_owned()).unwrap();
        unsafe {
            assert_eq!(
                client_mkdir(client, dir_path_cstring.as_ptr(), true),
                0,
                "Папка не была создана {dir_path_cstring:?}"
            );
        }
    }
    unsafe {
        info!("Работа в корнейвой дирекотрии {root_cstring:?}");
        let iter = client_ls(client, root_cstring.as_ptr());

        assert!(
            client_ls_has_next(iter),
            "Папка не пуста и должна содержать элементы."
        );
        let mut files = vec![];
        loop {
            let item = client_ls_next(iter);
            let Some(file) = ptr_to_string(item) else {
                debug!("Пустой элемент, завершаем чтение");
                break;
            };
            free_c_string(item);
            files.push(file);
        }
        assert_eq!(
            files.len(),
            11,
            "Ожидалось 11 элементов, получено {files:?}"
        );

        client_ls_free(iter);
    }

    // Проверка чтения пустой директории
    unsafe {
        let iter = client_ls(client, a2_cstring.as_ptr());
        let path = client_ls_next(iter);
        assert!(
            path.is_null(),
            "Папка пуста и должна быть пуста, но получено {path:?}"
        );
        assert!(!client_ls_has_next(iter), "Папка должна быть пустой");
        client_ls_free(iter);
    }

    // === тестирование удаления
    unsafe {
        assert_eq!(
            client_rmdir(client, a1_cstring.as_ptr(), false),
            3,
            "Папка не должна была быть удалена, т.к. она не пуста"
        );

        assert_eq!(
            client_rmdir(client, a2_cstring.as_ptr(), false),
            0,
            "Папка пуста и должна быть удалена"
        );

        assert!(
            !client_exists(client, a2_cstring.as_ptr()),
            "Папка была удалена {a2_cstring:?}"
        );
    }

    // === тестирование рекурсивного удаления
    unsafe {
        assert_eq!(
            client_rmdir(client, a_cstring.as_ptr(), true),
            0,
            "Папка должна быть удалена рекурсивно"
        );

        for dir_cstring in [&a_cstring, &a1_cstring] {
            assert!(
                !client_exists(client, dir_cstring.as_ptr()),
                "Папка не должна существовать {dir_cstring:?}"
            );
        }
    }

    // Отключение
    unsafe { client_disconnect(client) };
}
