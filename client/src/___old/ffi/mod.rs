// /// Преобразует указатель на C-строку в Rust строку.
// ///
// /// Возвращает `None`, если указатель равен `NULL`.
// ///
// /// # Safety
// ///
// /// При преобразование происходит проверка на NULL.
// pub unsafe fn ptr_to_string(cstr_ptr: *const c_char) -> Option<String> {
//     if cstr_ptr.is_null() {
//         return None;
//     }

//     let c_str = unsafe { CStr::from_ptr(cstr_ptr) };

//     c_str.to_str().ok().map(|s| s.to_string())
// }

// use fs4me_interface::ptr_to_string;

// use crate::{LocalDriver, interface::OldDriver};
// use std::ffi::{CString, c_char, c_void};

// pub mod dir;
// pub mod ls;

// // ===============================================================================
// // Для взаимодействия по FFI
// //
// // @todo
// // Эта часть будет общая для всех драйверов.
// // Нужно подумать, как сделать её через макрос
// // ===============================================================================

// /// Подключение к удаленному файловому хранилищу через FFI.
// ///
// /// @param params_ptr Указатель на C-строку с параметрами подключения. Формат: "key=value\nkey2=value2\n..."
// /// @return Указатель на Подключение к удаленному файловому хранилищу (LocalDriver) или NULL в случае ошибки.
// ///
// /// # Safety
// ///
// /// Функция возвращает указатель на клиент, который нужно освободить через `client_disconnect()`.
// #[unsafe(no_mangle)]
// pub unsafe extern "C" fn client_connect(params_ptr: *const c_char) -> *mut c_void {
//     unsafe {
//         let params_str = match ptr_to_string(params_ptr) {
//             Some(s) => s,
//             None => return std::ptr::null_mut(),
//         };

//         // 4. Логика работы с драйвером
//         match LocalDriver::connect(params_str) {
//             Ok(client) => {
//                 // Box::into_raw передает владение памятью вызывающей стороне.
//                 // Память будет освобождена через отдельную функцию free_driver().
//                 Box::into_raw(Box::new(client)) as *mut _
//             }
//             Err(err) => {
//                 println!("Error: {err}");
//                 // В случае внутренней ошибки драйвера возвращаем NULL
//                 std::ptr::null_mut()
//             }
//         }
//     }
// }

// /// Отключиться от файлового хранилища. Обязательная процедура. Необходима для высвобождения ресурсов.
// ///
// /// @param client_ptr Указатель на клиентский объект (LocalDriver).
// ///
// /// # Safety
// ///
// /// Функция ожидает указатель, полученный из `Box::into_raw(LocalDriver)`.
// /// Вызов функции с недопустимым указателем приведёт к неопределённому поведению.
// #[unsafe(no_mangle)]
// pub unsafe extern "C" fn client_disconnect(client_ptr: *mut c_void) {
//     // 1. Проверяем на NULL
//     if client_ptr.is_null() {
//         return;
//     }

//     // 2. Получаем указатель на драйвер из Box и освобождаем память.
//     unsafe { drop(Box::from_raw(client_ptr as *mut LocalDriver)) };
// }

// /// Вывести информацию о драйвере.
// ///
// /// @param client_ptr Указатель на клиентский объект (LocalDriver).
// /// @return Указатель на строку с информацией о драйвере.
// ///
// /// # Safety
// ///
// /// Функция возвращает указатель на строку, которую нужно освободить через `free_c_string()`.
// #[unsafe(no_mangle)]
// pub unsafe extern "C" fn client_get_info(client_ptr: *mut c_void) -> *mut c_char {
//     let client: &LocalDriver = match client_ptr.try_into() {
//         Ok(client) => client,
//         Err(err) => {
//             println!("Error: {}", err);
//             return std::ptr::null_mut();
//         }
//     };

//     let client_info_str = client.info();

//     match CString::new(client_info_str) {
//         Ok(c_string) => c_string.into_raw(),
//         Err(_) => std::ptr::null_mut(),
//     }
// }

// /// Освободить строку, выделенную для хранения информации о драйвере.
// ///
// /// @param ptr Указатель на строку, которую нужно освободить.
// ///
// /// # Safety
// ///
// /// Функция ожидает указатель, полученный из `CString::into_raw()`.
// /// Вызов функции с недопустимым указателем приведёт к неопределённому поведению.
// #[unsafe(no_mangle)]
// pub unsafe extern "C" fn free_c_string(ptr: *mut c_char) {
//     if !ptr.is_null() {
//         // Восстанавливаем владение CString и освобождаем память
//         unsafe { drop(CString::from_raw(ptr)) };
//     }
// }

// /// Получить текущее время сервера.
// ///
// /// @param client_ptr Указатель на клиентский объект (LocalDriver).
// /// @return Текущее время сервера в Unix-времени (секунды с начала эпохи).
// ///
// /// # Safety
// ///
// /// Указатель `client_ptr` должен быть не нулевым.
// #[unsafe(no_mangle)]
// pub unsafe extern "C" fn client_server_time(client_ptr: *mut c_void) -> u32 {
//     if client_ptr.is_null() {
//         return 0;
//     }

//     let client = unsafe { &*(client_ptr as *const LocalDriver) };

//     client.server_time().unwrap_or(0)
// }

// /// Проверить существование файла по указанному пути.
// ///
// /// @param client_ptr Указатель на клиентский объект (LocalDriver).
// /// @param path Указатель на строку с путем к файлу.
// /// @return true, если файл существует, false в противном случае.
// ///
// /// # Safety
// ///
// /// Указатель `client_ptr` должен быть не нулевым.
// /// Указатель `path` должен быть не нулевым и содержать корректную строку пути.
// #[unsafe(no_mangle)]
// pub unsafe extern "C" fn client_exists(client_ptr: *mut c_void, path: *const c_char) -> bool {
//     if client_ptr.is_null() {
//         return false;
//     }

//     let client = unsafe { &*(client_ptr as *const LocalDriver) };

//     let path = unsafe { ptr_to_string(path) };
//     let Some(path) = path else {
//         println!("Ошибка при преобразовании пути из указателя");
//         return false;
//     };

//     client.exists(&path)
// }
