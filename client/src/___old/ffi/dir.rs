// use std::{ffi::c_char, os::raw::c_void};

// use crate::{LocalDriver, ffi::ptr_to_string, interface::OldDriver};

// /// Создает директорию.
// ///
// /// @client_ptr - Указатель на клиентский объект.
// /// @param path - Путь к директории.
// /// @param recursive - Рекурсивное создание. Создает все промежуточные директории.
// /// @return Код ошибки:
// ///  0 - успех,
// ///  1 - ошибка при преобразовании указателя на клиентский объект,
// ///  2 - ошибка при преобразовании указателя на путь,
// ///  3 - ошибка при создании директории.
// ///
// /// # Safety
// ///
// /// Указатель `client_ptr` должен быть не нулевым.
// /// Указатель `path` должен быть не нулевым и содержать корректную строку пути.
// #[unsafe(no_mangle)]
// pub unsafe extern "C" fn client_mkdir(
//     client_ptr: *mut c_void,
//     path: *const c_char,
//     recursive: bool,
// ) -> i32 {
//     unsafe {
//         let client: &LocalDriver = match client_ptr.try_into() {
//             Ok(client) => client,
//             Err(err) => {
//                 println!("Error: {err}");
//                 return 1;
//             }
//         };
//         let path = match ptr_to_string(path) {
//             Some(path) => path,
//             None => {
//                 println!("Error: path is null");
//                 return 2;
//             }
//         };

//         match client.mkdir(&path, recursive) {
//             Ok(_) => 0,
//             Err(err) => {
//                 println!("Error: {err}");
//                 3
//             }
//         }
//     }
// }

// /// Удаляет директорию.
// ///
// /// @client_ptr - Указатель на клиентский объект.
// /// @param path - Путь к директории.
// /// @param recursive - Рекурсивное удаление. Удаляет все содержимое директории.
// /// @return Код ошибки:
// ///  0 - успех,
// ///  1 - ошибка при преобразовании указателя на клиентский объект,
// ///  2 - ошибка при преобразовании указателя на путь,
// ///  3 - ошибка при удалении директории.
// ///
// /// # Safety
// ///
// /// Указатель `client_ptr` должен быть не нулевым.
// /// Указатель `path` должен быть не нулевым и содержать корректную строку пути.
// ///
// #[unsafe(no_mangle)]
// pub unsafe extern "C" fn client_rmdir(client_ptr: *mut c_void, path: *const c_char) -> i32 {
//     unsafe {
//         let client: &LocalDriver = match client_ptr.try_into() {
//             Ok(client) => client,
//             Err(err) => {
//                 println!("Error: {err}");
//                 return 1;
//             }
//         };
//         let path = match ptr_to_string(path) {
//             Some(path) => path,
//             None => {
//                 println!("Error: path is null");
//                 return 2;
//             }
//         };
//         match client.rm(&path) {
//             Ok(_) => 0,
//             Err(err) => {
//                 println!("Error: {err}");
//                 3
//             }
//         }
//     }
// }
