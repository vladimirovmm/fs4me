// use crate::LocalDriver;
// use crate::ffi::ptr_to_string;
// use crate::interface::OldDriver;
// use std::ffi::{CString, c_char, c_void};
// use std::path::PathBuf;

// // Структура для управления итератором директории
// #[repr(C)]
// pub struct LsIteratorHandle {
//     iterator: Option<Box<dyn Iterator<Item = PathBuf> + Send + Sync>>,
// }

// /// Вывести содержимое директории.
// ///
// /// @param client_ptr Указатель на клиентский объект (LocalDriver).
// /// @param path_ptr Указатель на C-строку с путём к директории.
// /// @return Указатель на итератор директории (LsIteratorHandle).
// ///
// /// # Safety
// ///
// /// Возвращаемый указатель нужно освободить через `ls_free()`.
// #[unsafe(no_mangle)]
// pub unsafe extern "C" fn client_ls(
//     client_ptr: *mut c_void,
//     path_ptr: *const c_char,
// ) -> *mut LsIteratorHandle {
//     unsafe {
//         let client: &LocalDriver = match client_ptr.try_into() {
//             Ok(client) => client,
//             Err(err) => {
//                 println!("{:?}", err);
//                 return std::ptr::null_mut();
//             }
//         };

//         let path_str = match ptr_to_string(path_ptr) {
//             Some(path_str) => path_str,
//             None => return std::ptr::null_mut(),
//         };

//         // 4. Логика работы с драйвером
//         let Ok(iterator) = client.ls(path_str) else {
//             return std::ptr::null_mut();
//         };

//         // Создаем новую структуру итератора
//         let handle = Box::new(LsIteratorHandle {
//             iterator: Some(Box::new(iterator)),
//         });
//         Box::into_raw(handle)
//     }
// }

// /// Получить следующую строку из итератора директории.
// ///
// /// @param handle Указатель на итератор директории.
// /// @return Указатель на строку с именем файла или NULL, если итератор кончился.
// ///
// /// # Safety
// ///
// /// Возвращаемый указатель нужно освободить через `free_c_string()`.
// #[unsafe(no_mangle)]
// pub unsafe extern "C" fn client_ls_next(handle: *mut LsIteratorHandle) -> *mut c_char {
//     unsafe {
//         if handle.is_null() {
//             return std::ptr::null_mut();
//         }

//         let handle = &mut *handle;

//         if let Some(ref mut iterator) = handle.iterator
//             && let Some(path) = iterator.next()
//         {
//             let path_str = path.to_string_lossy();
//             // Создаём CString — он владеет памятью
//             let c_string = CString::new(path_str.as_ref()).unwrap_or_else(|_| CString::default());
//             // Передаём владение указателем наружу
//             return c_string.into_raw();
//         }

//         // Итератор кончился, очищаем handle
//         handle.iterator = None;
//         std::ptr::null_mut()
//     }
// }

// /// Проверить, остались ли элементы в итераторе.
// ///
// /// @param handle Указатель на итератор директории.
// /// @return TRUE, если элементы остались, FALSE, если итератор кончился.
// ///
// /// # Safety
// ///
// /// Возвращаемый указатель нужно освободить через `ls_free()`.
// #[unsafe(no_mangle)]
// pub unsafe extern "C" fn client_ls_has_next(handle: *mut LsIteratorHandle) -> bool {
//     unsafe {
//         if handle.is_null() {
//             return false;
//         }

//         let handle = &*handle;
//         handle.iterator.is_some()
//     }
// }

// /// Освободить итератор директории.
// ///
// /// @param handle Указатель на итератор директории.
// ///
// /// # Safety
// ///
// /// Вызов функции с недопустимым указателем приведёт к неопределённому поведению.
// #[unsafe(no_mangle)]
// pub unsafe extern "C" fn client_ls_free(handle: *mut LsIteratorHandle) {
//     unsafe {
//         if !handle.is_null() {
//             drop(Box::from_raw(handle));
//         }
//     }
// }
