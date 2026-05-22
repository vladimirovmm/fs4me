#![cfg(test)]

//! # Модуль для тестирования FFI-интерфейса
//!
//! Этот модуль содержит интеграционные тесты для FFI-функций локального
//! файлового хранилища. Функции вызываются через динамически загрузимую
//! библиотеку `fs4me_local`.
//!
//! ## Загружаемые функции
//!
//! - [`client_connect`](client_connect) — подключение к файловому хранилищу
//! - [`client_disconnect`](client_disconnect) — отключение от хранилища
//! - [`client_get_info`](client_get_info) — получение информации о драйвере
//! - [`client_server_time`](client_server_time) — получение времени сервера
//! - [`free_c_string`](free_c_string) — освобождение C-строки

use std::{
    ffi::{CStr, CString, c_char, c_void},
    path::Path,
    str::FromStr,
    time::{SystemTime, UNIX_EPOCH},
};

use chrono::{DateTime, Local, Utc};
use tracing_test::traced_test;

mod dir;

#[link(name = "fs4me_local")]
unsafe extern "C" {
    /// Подключение к удаленному файловому хранилищу через FFI.
    ///
    /// @param params_ptr Указатель на C-строку с параметрами подключения. Формат: "key=value\nkey2=value2\n..."
    /// @return Указатель на Подключение к удаленному файловому хранилищу (LocalDriver) или NULL в случае ошибки.
    pub fn client_connect(params_ptr: *const c_char) -> *mut c_void;

    /// Отключиться от файлового хранилища. Обязательная процедура. Необходима для высвобождения ресурсов.
    ///
    /// @param client_ptr Указатель на клиентский объект (LocalDriver).
    pub fn client_disconnect(client_ptr: *mut c_void);

    /// Вывести информацию о драйвере.
    ///
    /// @param client_ptr Указатель на клиентский объект (LocalDriver).
    /// @return Указатель на строку с информацией о драйвере.
    pub fn client_get_info(client_ptr: *mut c_void) -> *mut c_char;

    /// Освободить строку, выделенную для хранения информации о драйвере.
    ///
    /// @param client_ptr Указатель на клиентский объект (`LocalDriver`)
    /// @return UNIX timestamp в секундах
    pub fn client_server_time(client_ptr: *mut c_void) -> u32;

    /// Освобождение памяти для C-строки, выделенной FFI-функцией.
    ///
    /// @param ptr Указатель на C-строку для освобождения
    pub fn free_c_string(ptr: *const c_char);

    /// Получить список файлов на сервере.
    ///
    /// @param client_ptr Указатель на клиентский объект (`LocalDriver`)
    /// @param path Путь к директории для получения списка файлов
    /// @return Указатель на итератор списка файлов
    pub fn client_ls(client_ptr: *mut c_void, path: *const c_char) -> *mut c_void;

    /// Получить следующий элемент из итератора списка файлов.
    ///
    /// @param handle Указатель на итератор списка файлов
    /// @return Указатель на следующий элемент списка файлов. Необходимо освободить память с помощью `free_c_string`.
    pub fn client_ls_next(handle: *mut c_void) -> *mut c_char;

    /// Проверить, есть ли следующий элемент в итераторе списка файлов.
    ///
    /// @param handle Указатель на итератор списка файлов
    /// @return `true`, если есть следующий элемент, иначе `false`
    pub fn client_ls_has_next(handle: *mut c_void) -> bool;

    /// Освободить память для итератора списка файлов.
    ///
    /// @param handle Указатель на итератор списка файлов
    pub fn client_ls_free(handle: *mut c_void);

    /// Проверить существование файла по указанному пути.
    ///
    /// @param client_ptr Указатель на клиентский объект (LocalDriver).
    /// @param path Указатель на строку с путем к файлу.
    /// @return `true`, если файл существует, иначе `false`.
    pub fn client_exists(client_ptr: *mut c_void, path: *const c_char) -> bool;

    /// Создает директорию.
    ///
    /// @client_ptr - Указатель на клиентский объект.
    /// @param path - Путь к директории.
    /// @param recursive - Рекурсивное создание. Создает все промежуточные директории.
    /// @return Код ошибки:
    ///    0 - успех,
    ///    1 - ошибка при преобразовании указателя на клиентский объект,
    ///    2 - ошибка при преобразовании указателя на путь,
    ///    3 - ошибка при создании директории.
    pub fn client_mkdir(client_ptr: *mut c_void, path: *const c_char, recursive: bool) -> i32;

    /// Удаляет директорию.
    ///
    /// @client_ptr - Указатель на клиентский объект.
    /// @param path - Путь к директории.
    /// @param recursive - Рекурсивное удаление. Удаляет все содержимое директории.
    /// @return Код ошибки:
    ///    0 - успех,
    ///    1 - ошибка при преобразовании указателя на клиентский объект,
    ///    2 - ошибка при преобразовании указателя на путь,
    ///    3 - ошибка при удалении директории.
    ///
    pub fn client_rmdir(client_ptr: *mut c_void, path: *const c_char, recursive: bool) -> i32;
}

fn path_to_cstring<P: AsRef<Path>>(path: P) -> CString {
    CString::from_str(path.as_ref().to_str().unwrap()).unwrap()
}

#[test]
#[traced_test]
fn test_info() {
    let params_cstring = std::ffi::CString::new("").unwrap();

    // Подключение
    let client = unsafe { client_connect(params_cstring.as_ptr()) };
    assert!(!client.is_null(), "Подключение не удалось");

    // Получение информации
    let info_ptr = unsafe { client_get_info(client) };
    assert!(!info_ptr.is_null(), "Информация о драйвере не получена");

    let info_str = unsafe { CStr::from_ptr(info_ptr) }
        .to_string_lossy()
        .to_string();
    // Освобождение памяти для строки информации
    unsafe { free_c_string(info_ptr) };

    println!();
    println!("Информация о драйвере:\n{}", info_str);
    println!();

    assert!(
        info_str.contains("name="),
        "name не найден в информации о драйвере"
    );
    assert!(
        info_str.contains("version="),
        "version не найден в информации о драйвере"
    );

    // Отключение клиента
    unsafe { client_disconnect(client) };
}

#[test]
#[traced_test]
fn test_server_time() {
    let params_cstring = std::ffi::CString::new("").unwrap();

    // Подключение
    let client = unsafe { client_connect(params_cstring.as_ptr()) };
    assert!(!client.is_null(), "Подключение не удалось");

    // Получение времени сервера
    let server_time = unsafe { client_server_time(client) };
    assert!(server_time > 0, "Время сервера не получено");
    let local_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as u32;

    assert!(
        local_time >= server_time,
        "Время сервера должно быть меньше или равно локальному времени\n\
        Локальное время: {local_time}\n\
        Время сервера: {server_time}"
    );

    let server_time = DateTime::from_timestamp_secs(server_time as i64).unwrap();

    println!(
        "Текущее время сервера: \n\
        Время UTC: {}\n\
        Локальное время: {}",
        server_time.with_timezone(&Utc).format("%d.%m.%Y %H:%M:%S"),
        server_time
            .with_timezone(&Local)
            .format("%d.%m.%Y %H:%M:%S")
    );

    // Отключение
    unsafe { client_disconnect(client) };
}
