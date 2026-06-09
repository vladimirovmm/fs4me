extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, parse_macro_input};

/// Макрос для генерации FFI‑интерфейса для драйверов
#[proc_macro_derive(DriverFFI)]
pub fn driver_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let struct_name = &input.ident;

    let expanded = quote! {
        pub mod ffi {
            use super::*;
            use fs4me_client::Fs;
            use fs4me_client::buffer::{DriverBufferRead, DriverBufferWrite};
            use std::ffi::{
                CStr, CString, c_char, c_int, c_long, c_schar, c_uchar, c_uint, c_ulong, c_void,
            };
            use std::io::Write;
            use std::sync::Arc;
            use tracing::{error, warn};

            fn copy_to_c_char(src: &str, error_cstr: *mut c_char, error_size: c_uint) {
                if error_cstr.is_null() || error_size == 0 {
                    return;
                }

                let bytes = src.as_bytes();
                let required_size = bytes.len() + 1; // +1 для \0

                if required_size > error_size as usize {
                    unsafe {
                        *error_cstr = 0;
                    }
                    return;
                }

                // Копируем все байты сразу
                unsafe {
                    std::ptr::copy_nonoverlapping(bytes.as_ptr() as *const c_char, error_cstr, bytes.len());
                    // Добавляем нулевой терминатор
                    *error_cstr.add(bytes.len()) = 0;
                }
            }

            fn c_char_to_string(c_str: *const c_char) -> Option<String> {
                if c_str.is_null() {
                    return None;
                }
                unsafe { CStr::from_ptr(c_str) }
                    .to_str()
                    .map(|s| s.to_string())
                    .ok()
            }

            fn err_to_string(err: impl std::fmt::Display) -> String {
                err.to_string()
            }

            /// Освободить память, выделенную для строки информации о клиенте
            ///
            /// # Safety
            ///
            /// Параметр `info_ptr` должен быть не `null` и содержать корректные данные, полученные через `CString::into_raw`
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn client_cstring_free(info_ptr: *mut c_char) {
                if info_ptr.is_null() {
                    return;
                }

                // Восстанавливаем владение и освобождаем
                let _ = unsafe { CString::from_raw(info_ptr) };
                // CString автоматически освободит память при выходе из области видимости
            }

            /// # Safety
            ///
            /// Параметр `params` должен быть не `null` и содержать корректные данные
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn client_connect(
                params: *const c_char,
                error: *mut c_char,
                error_size: c_uint,
            ) -> *const Fs<#struct_name> {
                let Some(params_str) = c_char_to_string(params) else {
                    copy_to_c_char("params is null", error, error_size);
                    return std::ptr::null();
                };

                match #struct_name::connect(params_str).map(Fs::new) {
                    Ok(client) => Arc::into_raw(Arc::new(client)),
                    Err(err) => {
                        copy_to_c_char(&err.to_string(), error, error_size);
                        std::ptr::null()
                    }
                }
            }

            /// # Safety
            ///
            /// Параметр `client` должен быть не `null` и содержать корректные данные, полученные через `Arc::into_raw`
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn client_free(client: *const Fs<#struct_name>) -> c_int {
                if client.is_null() {
                    warn!("client is null");
                    return -1;
                }

                // Восстанавливаем владение Arc и явно вызываем drop
                let _arc_client = unsafe { Arc::from_raw(client) };

                0
            }

            /// Получить информацию о клиенте в виде строки
            ///
            /// # Safety
            ///
            /// Параметр `client` должен быть не `null` и содержать корректные данные, полученные через `Arc::into_raw`
            /// Необходимо освободить память через `client_cstring_free`
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn client_info(client: *const Fs<#struct_name>) -> *mut c_char {
                if client.is_null() {
                    warn!("client is null");
                    return std::ptr::null_mut();
                }

                let client_ref = unsafe { &*client };
                let c_str = match CString::new(client_ref.driver_info()) {
                    Ok(s) => s,
                    Err(_) => {
                        warn!("failed to create CString");
                        return std::ptr::null_mut();
                    }
                };

                c_str.into_raw() // передаём владение C‑коду
            }

            /// Получить время последнего успешного подключения клиента
            ///
            /// # Safety
            ///
            /// Параметр `client` должен быть не `null` и содержать корректные данные, полученные через `Arc::into_raw`
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn client_time(client: *const Fs<#struct_name>) -> c_ulong {
                if client.is_null() {
                    warn!("client is null");
                    return 0;
                }
                let client = unsafe { &*client };

                match client.time() {
                    Ok(time) => time.as_secs(),
                    Err(err) => {
                        warn!("client time error: {}", err);
                        0
                    }
                }
            }

            /// Проверить существование пути
            ///
            /// # Safety
            ///
            /// Параметр `client` должен быть не `null` и содержать корректные данные, полученные через `Arc::into_raw`
            /// Параметр `path` должен быть не `null` и содержать корректную строку
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn client_exist(
                client: *const Fs<#struct_name>,
                path: *const c_char,
            ) -> c_schar {
                if client.is_null() {
                    warn!("client is null");
                    return -1;
                }
                let Some(path_str) = c_char_to_string(path) else {
                    warn!("path is null");
                    return -2;
                };

                let client = unsafe { &*client };

                client.exists(&path_str) as i8
            }

            /// Создает директорию
            ///
            /// # Safety
            ///
            /// `client` должен быть не `null` и указывать на корректный клиент.
            /// `path` должен быть не `null` и указывать на корректный путь.
            /// `recursive` указывает, нужно ли рекурсивно создавать директории по пути.
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn client_mkdir(
                client: *const Fs<#struct_name>,
                path: *const c_char,
                recursive: c_int,
            ) -> c_schar {
                if client.is_null() {
                    warn!("client is null");
                    return -1;
                }

                let Some(path_str) = c_char_to_string(path) else {
                    warn!("path is null");
                    return -2;
                };

                let client = unsafe { &*client };

                match client.mkdir(path_str, recursive != 0) {
                    Ok(_) => 0,
                    Err(e) => {
                        error!("mkdir error: {e}");
                        -3
                    }
                }
            }

            /// Открывает файл для записи и возвращает указатель на `Box<dyn io::Write>`.
            ///
            ///
            /// # Safety
            ///
            /// `client` должен быть не `null` и указывать на корректный клиент.
            /// `path` должен быть не `null` и указывать на корректный путь.
            /// `mode` должен быть не `null` и указывать на корректные данные. Ожидается `fail_if_exist`, `overwrite`, `append`
            ///
            /// Освободить память через `client_write_close`
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn client_write_open(
                client: *const Fs<#struct_name>,
                path: *const c_char,
                mode: *const c_char,
                error: *mut c_char,
                error_size: c_uint,
            ) -> *mut DriverBufferWrite<#struct_name> {
                if client.is_null() {
                    copy_to_c_char("client is null", error, error_size);
                    return std::ptr::null_mut();
                }

                let Some(path_str) = c_char_to_string(path) else {
                    copy_to_c_char("path is null", error, error_size);
                    return std::ptr::null_mut();
                };

                let Some(mode_str) = c_char_to_string(mode) else {
                    copy_to_c_char("mode is null", error, error_size);
                    return std::ptr::null_mut();
                };
                let Ok(mode) = mode_str.parse::<WriteMode>() else {
                    copy_to_c_char("invalid mode", error, error_size);
                    return std::ptr::null_mut();
                };

                let client = unsafe { &*client };

                match client.write(&path_str, mode) {
                    Ok(writer) => Box::into_raw(writer),
                    Err(err) => {
                        let err = err.to_string();
                        copy_to_c_char(err.as_str(), error, error_size);

                        std::ptr::null_mut()
                    }
                }
            }

            /// Закрывает `writer`, освобождая ресурсы.
            ///
            /// # Safety
            ///
            /// `writer` должен быть не `null` и указывать на корректный writer.
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn client_write_close(
                writer: *mut DriverBufferWrite<#struct_name>,
            ) -> c_schar {
                if writer.is_null() {
                    warn!("writer is null");
                    return -1;
                }

                let writer_box = unsafe { Box::from_raw(writer) };
                drop(writer_box);
                0
            }

            /// Записывает `size` байт данных из `data` в `writer`.
            ///
            /// # Safety
            /// - `writer` должен быть не `null` и указывать на корректный `Box<dyn io::Write>`.
            /// - `data` должен быть не `null` и указывать на валидную память, содержащую `size` байт данных.
            /// - `size` должен быть больше `0`.
            /// - Вызывающий код должен гарантировать, что память, на которую указывает `data`, не будет изменена или освобождена во время выполнения операции записи.
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn client_write(
                writer: *mut DriverBufferWrite<#struct_name>,
                data: *const c_uchar, // теперь явно u8 вместо c_char
                size: c_ulong,
            ) -> c_schar {
                if writer.is_null() {
                    warn!("writer is null");
                    return -1;
                }
                if data.is_null() {
                    warn!("data is null");
                    return -2;
                }
                if size == 0 {
                    warn!("size is zero");
                    return -3;
                }

                // Преобразуем указатель и длину в срез &[u8]
                let data_slice = unsafe { std::slice::from_raw_parts(data, size as usize) };

                // Восстанавливаем writer из сырого указателя для вызова методов
                let writer_ref = unsafe { &mut *writer };

                match writer_ref.write_all(data_slice) {
                    Ok(_) => 0,
                    Err(err) => {
                        error!("write error: {err:?}");
                        -4
                    }
                }
            }

            /// Читает содержимое файла и возвращает его как `c_char` строку.
            ///
            /// # Safety
            ///
            /// `client` должен быть не `null` и указывать на корректный клиент.
            /// `path` должен быть не `null` и указывать на корректный путь.
            /// `error` должен быть не `null` и указывать на корректный буфер для ошибок.
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn client_read(
                client: *const Fs<#struct_name>,
                path: *const c_char,
                position: c_ulong,
                error: *mut c_char,
                error_size: c_uint,
            ) -> *mut DriverBufferRead<#struct_name> {
                if client.is_null() {
                    copy_to_c_char("client is null", error, error_size);
                    return std::ptr::null_mut();
                }

                let Some(path_str) = c_char_to_string(path) else {
                    copy_to_c_char("path is null", error, error_size);
                    return std::ptr::null_mut();
                };

                let client = unsafe { &*client };

                match client.read(&path_str, position) {
                    Ok(reader) => Box::into_raw(reader),
                    Err(err) => {
                        copy_to_c_char(&err_to_string(err), error, error_size);
                        std::ptr::null_mut()
                    }
                }
            }

            /// Читает данные из читателя и записывает их в буфер
            ///
            /// # Safety
            ///
            /// `reader` - указатель на `DriverBufferReed`
            /// `buffer` - указатель на буфер для записи
            /// `size` - размер буфера
            ///
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn client_read_to_buffer(
                reader: *mut DriverBufferRead<#struct_name>,
                buffer: *mut c_uchar,
                size: c_uint,
            ) -> c_long {
                // Проверка на нулевые указатели
                if reader.is_null() || buffer.is_null() {
                    error!("null pointer provided");
                    return -1;
                }

                // Создание ссылки на reader
                let reader_ref = unsafe { &mut *reader };

                // Создание среза буфера с проверкой валидности
                let buffer_slice = unsafe { std::slice::from_raw_parts_mut(buffer, size as usize) };

                // Чтение данных
                match reader_ref.read(buffer_slice) {
                    Ok(size) => size as i64,
                    Err(err) => {
                        error!("read error: {err:?}");
                        -4
                    }
                }
            }

            /// Закрыть читатель
            ///
            /// # Safety
            ///
            /// Параметр `reader` должен быть не `null` и содержать корректные данные, полученные через `Box::into_raw`
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn client_read_close(reader: *mut DriverBufferRead<#struct_name>) {
                unsafe {
                    let _ = Box::from_raw(reader);
                }
            }

            /// Получить список файлов в директории
            ///
            /// # Safety
            ///
            /// Параметр `client` должен быть не `null` и содержать корректные данные, полученные через `Arc::into_raw`
            /// Параметр `path` должен быть не `null` и содержать корректную строку
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn client_ls(
                client: *const Fs<#struct_name>,
                path: *const c_char,
                error: *mut c_char,
                error_size: c_uint,
            ) -> *mut c_void {
                if client.is_null() {
                    copy_to_c_char("client is null", error, error_size);
                    return std::ptr::null_mut();
                }
                let Some(path_str) = c_char_to_string(path) else {
                    copy_to_c_char("path is null", error, error_size);
                    return std::ptr::null_mut();
                };
                let client = unsafe { &*client };
                match client.ls(path_str) {
                    Ok(iter) => {
                        let dyn_iter: Box<Box<dyn Iterator<Item = PathBuf> + 'static>> =
                            Box::new(Box::new(iter));
                        Box::into_raw(dyn_iter) as *mut c_void
                    }
                    Err(err) => {
                        copy_to_c_char(&err.to_string(), error, error_size);
                        std::ptr::null_mut()
                    }
                }
            }

            /// Возвращает следующий элемент из итератора, полученного через `client_ls`
            ///
            /// # Safety
            ///
            /// `iter_ptr` должен быть не `null` и указывать на корректный итератор.
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn client_ls_next(iter_ptr: *mut c_void) -> *mut c_char {
                if iter_ptr.is_null() {
                    return std::ptr::null_mut();
                }

                // Предполагается, что iter_ptr указывает на Box<dyn Iterator<Item = PathBuf> + 'static>
                let iter: &mut Box<dyn Iterator<Item = PathBuf> + 'static> =
                    unsafe { &mut *(iter_ptr as *mut Box<dyn Iterator<Item = PathBuf> + 'static>) };

                let next = iter.next();
                match next {
                    Some(path) => {
                        // Используем display() для получения строкового представления пути
                        let c_str = CString::new(path.display().to_string())
                            .expect("Failed to convert path to CString");
                        c_str.into_raw()
                    }
                    None => std::ptr::null_mut(),
                }
            }

            /// Освобождает память, выделенную для итератора, полученного через `client_ls`
            ///
            /// # Safety
            ///
            /// `iter_ptr` должен быть не `null` и указывать на корректный итератор.
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn client_ls_free(iter_ptr: *mut c_void) {
                if iter_ptr.is_null() {
                    return;
                }

                let boxed_iter = iter_ptr as *mut Box<dyn Iterator<Item = PathBuf> + 'static>;
                // Восстанавливаем Box и освобождаем память
                let _ = unsafe { Box::from_raw(boxed_iter) };
            }

            /// Возвращает статистику для файла или директории
            ///
            /// # Safety
            ///
            /// `client` должен быть не `null` и указывать на корректный клиент.
            /// `path` должен быть не `null` и указывать на корректный путь.
            ///
            /// Необходимо освободить память через `client_cstring_free`
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn client_stat(
                client: *const Fs<#struct_name>,
                path: *const c_char,
                error: *mut c_char,
                error_size: c_uint,
            ) -> *mut c_char {
                if client.is_null() {
                    copy_to_c_char("client is null", error, error_size);
                    return std::ptr::null_mut();
                }

                let Some(path_str) = c_char_to_string(path) else {
                    copy_to_c_char("path is null", error, error_size);
                    return std::ptr::null_mut();
                };
                dbg!(&path_str);

                let client = unsafe { &*client };

                match client
                    .stat(path_str)
                    .map_err(err_to_string)
                    .map(|stat| stat.to_string())
                {
                    Ok(stat) => {
                        let stat_cstr = CString::new(stat).unwrap();
                        dbg!(&stat_cstr);
                        stat_cstr.into_raw()
                    }
                    Err(e) => {
                        dbg!(&e);
                        copy_to_c_char(&e, error, error_size);
                        std::ptr::null_mut()
                    }
                }
            }

            /// Перемещает файл или директорию
            ///
            /// # Safety
            ///
            /// `client` должен быть не `null` и указывать на корректный клиент.
            /// `src` и `dst` должны быть не `null` и указывать на корректные пути.
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn client_rename(
                client: *const Fs<#struct_name>,
                src: *const c_char,
                dst: *const c_char,
            ) -> c_schar {
                if client.is_null() {
                    error!("client is null");
                    return -1;
                }

                let Some(src_str) = c_char_to_string(src) else {
                    error!("src is null");
                    return -2;
                };

                let Some(dst_str) = c_char_to_string(dst) else {
                    error!("dst is null");
                    return -3;
                };

                let client = unsafe { &*client };

                match client.rename(src_str, dst_str) {
                    Ok(_) => 0,
                    Err(e) => {
                        error!("mv error: {e}");
                        -4
                    }
                }
            }

            /// Удаляет директорию или файл
            ///
            /// # Safety
            ///
            /// `client` должен быть не `null` и указывать на корректный клиент.
            /// `path` должен быть не `null` и указывать на корректный путь.
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn client_rm(
                client: *const Fs<#struct_name>,
                path: *const c_char,
            ) -> c_schar {
                if client.is_null() {
                    warn!("client is null");
                    return -1;
                }

                let Some(path_str) = c_char_to_string(path) else {
                    warn!("path is null");
                    return -2;
                };

                let client = unsafe { &*client };

                match client.rm(path_str) {
                    Ok(_) => 0,
                    Err(e) => {
                        error!("rm error: {e}");
                        -3
                    }
                }
            }
        }


    };

    expanded.into()
}
