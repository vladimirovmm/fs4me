#![cfg(test)]

use std::ffi::{CStr, c_char, c_void};

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
    pub fn client_get_info(client_ptr: *mut c_void) -> *const c_char;

    /// Освободить строку, выделенную для хранения информации о драйвере.
    ///
    /// @param ptr Указатель на строку, которую нужно освободить.
    pub fn free_c_string(ptr: *mut c_char);
}
#[test]
fn test_ffi_operations() {
    let params_str = "key=value\n";
    let params_cstring = std::ffi::CString::new(params_str).unwrap();

    // Подключение
    let client = unsafe { client_connect(params_cstring.as_ptr()) };
    assert!(!client.is_null(), "Подключение не удалось");

    // Получение информации
    let info_ptr = unsafe { client_get_info(client) };
    assert!(!info_ptr.is_null(), "Информация о драйвере не получена");

    // 123
    let info_str = unsafe { CStr::from_ptr(info_ptr) }
        .to_string_lossy()
        .to_string();
    // Освобождение памяти для строки информации
    unsafe { free_c_string(info_ptr as *mut c_char) };

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
