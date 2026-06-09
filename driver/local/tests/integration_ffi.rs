use std::{
    ffi::{CString, c_char, c_int, c_long, c_schar, c_uchar, c_uint, c_ulong, c_void},
    fs,
    path::PathBuf,
    process::Command,
};

use ctor::ctor;
use tempfile::tempdir;
use tracing::{debug, info};
use tracing_test::traced_test;

const ERROR_LEN: usize = 1000;

#[ctor(unsafe)]
fn init() {
    println!("Сборка библиотеки fs4me-local");
    let status = Command::new("cargo")
        .args(["build", "--lib", "-p", "fs4me-local"])
        .status()
        .unwrap();
    assert!(status.success());
    println!("Библиотека fs4me-local собрана");
}

type FnClientConnect<'lib> = libloading::Symbol<
    'lib,
    unsafe extern "C" fn(
        params: *const c_char,
        error: *mut c_char,
        error_size: c_uint,
    ) -> *const c_void,
>;
type FnClientDisconnect<'lib> =
    libloading::Symbol<'lib, unsafe extern "C" fn(client: *const c_void) -> c_int>;

struct DLibClient {
    lib: libloading::Library,
}

impl DLibClient {
    fn new(path_to_lib: &str) -> Self {
        let lib = unsafe { libloading::Library::new(path_to_lib).unwrap() };

        Self { lib }
    }

    fn functions<'lib>(&'lib self) -> DLibClientFn<'lib> {
        let client_connect: libloading::Symbol<
            'lib,
            unsafe extern "C" fn(
                params: *const c_char,
                error: *mut c_char,
                error_size: c_uint,
            ) -> *const c_void,
        > = unsafe { self.lib.get(b"client_connect").unwrap() };

        let client_free: libloading::Symbol<
            'lib,
            unsafe extern "C" fn(params: *const c_void) -> c_int,
        > = unsafe { self.lib.get(b"client_free").unwrap() };

        let client_cstring_free: libloading::Symbol<
            'lib,
            unsafe extern "C" fn(info_ptr: *mut c_char),
        > = unsafe { self.lib.get(b"client_cstring_free").unwrap() };

        let client_info: libloading::Symbol<
            'lib,
            unsafe extern "C" fn(client: *const c_void) -> *mut c_char,
        > = unsafe { self.lib.get(b"client_info").unwrap() };

        let client_time: libloading::Symbol<
            'lib,
            unsafe extern "C" fn(client: *const c_void) -> c_ulong,
        > = unsafe { self.lib.get(b"client_time").unwrap() };

        let client_exist: libloading::Symbol<
            'lib,
            unsafe extern "C" fn(client: *const c_void, path: *const c_char) -> c_schar,
        > = unsafe { self.lib.get(b"client_exist").unwrap() };

        let client_mkdir: libloading::Symbol<
            'lib,
            unsafe extern "C" fn(
                client: *const c_void,
                path: *const c_char,
                recursive: c_int,
            ) -> c_schar,
        > = unsafe { self.lib.get(b"client_mkdir").unwrap() };

        let client_write_open: libloading::Symbol<
            'lib,
            unsafe extern "C" fn(
                client: *const c_void,
                path: *const c_char,
                mode: *const c_char,
                error: *mut c_char,
                error_size: c_uint,
            ) -> *mut c_void,
        > = unsafe { self.lib.get(b"client_write_open").unwrap() };

        let client_write_close: libloading::Symbol<
            'lib,
            unsafe extern "C" fn(writer: *mut c_void) -> c_schar,
        > = unsafe { self.lib.get(b"client_write_close").unwrap() };

        let client_write: libloading::Symbol<
            'lib,
            unsafe extern "C" fn(writer: *mut c_void, data: *const u8, size: c_ulong) -> c_schar,
        > = unsafe { self.lib.get(b"client_write").unwrap() };

        let client_read: libloading::Symbol<
            'lib,
            unsafe extern "C" fn(
                client: *const c_void,
                path: *const c_char,
                position: c_ulong,
                error: *mut c_char,
                error_size: c_uint,
            ) -> *mut c_void,
        > = unsafe { self.lib.get(b"client_read").unwrap() };

        let client_read_to_buffer: libloading::Symbol<
            'lib,
            unsafe extern "C" fn(reader: *mut c_void, buffer: *mut c_uchar, size: c_uint) -> c_long,
        > = unsafe { self.lib.get(b"client_read_to_buffer").unwrap() };

        let client_read_close: libloading::Symbol<'lib, unsafe extern "C" fn(reader: *mut c_void)> =
            unsafe { self.lib.get(b"client_read_close").unwrap() };

        let client_ls: libloading::Symbol<
            'lib,
            unsafe extern "C" fn(
                client: *const c_void,
                path: *const c_char,
                error: *mut c_char,
                error_size: c_uint,
            ) -> *mut c_void,
        > = unsafe { self.lib.get(b"client_ls").unwrap() };

        let client_ls_next: libloading::Symbol<
            'lib,
            unsafe extern "C" fn(iter_ptr: *mut c_void) -> *mut c_char,
        > = unsafe { self.lib.get(b"client_ls_next").unwrap() };

        let client_ls_free: libloading::Symbol<'lib, unsafe extern "C" fn(iter_ptr: *mut c_void)> =
            unsafe { self.lib.get(b"client_ls_free").unwrap() };

        let client_stat: libloading::Symbol<
            'lib,
            unsafe extern "C" fn(
                client: *const c_void,
                path: *const c_char,
                error: *mut c_char,
                error_size: c_uint,
            ) -> *mut c_char,
        > = unsafe { self.lib.get(b"client_stat").unwrap() };

        let client_rename: libloading::Symbol<
            'lib,
            unsafe extern "C" fn(
                client: *const c_void,
                src: *const c_char,
                dst: *const c_char,
            ) -> c_schar,
        > = unsafe { self.lib.get(b"client_rename").unwrap() };

        let client_rm: libloading::Symbol<
            'lib,
            unsafe extern "C" fn(client: *const c_void, path: *const c_char) -> c_schar,
        > = unsafe { self.lib.get(b"client_rm").unwrap() };
        let client_clear_trash: libloading::Symbol<
            'lib,
            unsafe extern "C" fn(client: *const c_void, path: *const c_char) -> c_schar,
        > = unsafe { self.lib.get(b"client_clear_trash").unwrap() };

        let client_copy_file: libloading::Symbol<
            'lib,
            unsafe extern "C" fn(
                client: *const c_void,
                src: *const c_char,
                dst: *const c_char,
            ) -> c_schar,
        > = unsafe { self.lib.get(b"client_copy_file").unwrap() };
        let client_copy: libloading::Symbol<
            'lib,
            unsafe extern "C" fn(
                client: *const c_void,
                src: *const c_char,
                dst: *const c_char,
            ) -> c_schar,
        > = unsafe { self.lib.get(b"client_copy").unwrap() };

        DLibClientFn {
            connect: client_connect,
            disconnect: client_free,
            client_cstring_free,
            client_info,
            client_time,
            client_exist,
            client_mkdir,
            client_write_open,
            client_write_close,
            client_write,
            client_read,
            client_read_to_buffer,
            client_read_close,
            client_ls,
            client_ls_next,
            client_ls_free,
            client_stat,
            client_rename,
            client_rm,
            client_clear_trash,
            client_copy_file,
            client_copy,
        }
    }
}

struct DLibClientFn<'lib> {
    connect: FnClientConnect<'lib>,
    disconnect: FnClientDisconnect<'lib>,
    client_cstring_free: libloading::Symbol<'lib, unsafe extern "C" fn(info_ptr: *mut c_char)>,
    client_info:
        libloading::Symbol<'lib, unsafe extern "C" fn(client: *const c_void) -> *mut c_char>,
    client_time: libloading::Symbol<'lib, unsafe extern "C" fn(client: *const c_void) -> c_ulong>,
    client_exist: libloading::Symbol<
        'lib,
        unsafe extern "C" fn(client: *const c_void, path: *const c_char) -> c_schar,
    >,
    client_mkdir: libloading::Symbol<
        'lib,
        unsafe extern "C" fn(
            client: *const c_void,
            path: *const c_char,
            recursive: c_int,
        ) -> c_schar,
    >,
    client_write_open: libloading::Symbol<
        'lib,
        unsafe extern "C" fn(
            client: *const c_void,
            path: *const c_char,
            mode: *const c_char,
            error: *mut c_char,
            error_size: c_uint,
        ) -> *mut c_void,
    >,
    client_write_close:
        libloading::Symbol<'lib, unsafe extern "C" fn(writer: *mut c_void) -> c_schar>,
    client_write: libloading::Symbol<
        'lib,
        unsafe extern "C" fn(writer: *mut c_void, data: *const c_uchar, size: c_ulong) -> c_schar,
    >,
    client_read: libloading::Symbol<
        'lib,
        unsafe extern "C" fn(
            client: *const c_void,
            path: *const c_char,
            position: c_ulong,
            error: *mut c_char,
            error_size: c_uint,
        ) -> *mut c_void,
    >,
    client_read_to_buffer: libloading::Symbol<
        'lib,
        unsafe extern "C" fn(reader: *mut c_void, buffer: *mut c_uchar, size: c_uint) -> c_long,
    >,
    client_read_close: libloading::Symbol<'lib, unsafe extern "C" fn(reader: *mut c_void)>,
    client_ls: libloading::Symbol<
        'lib,
        unsafe extern "C" fn(
            client: *const c_void,
            path: *const c_char,
            error: *mut c_char,
            error_size: c_uint,
        ) -> *mut c_void,
    >,
    client_ls_next:
        libloading::Symbol<'lib, unsafe extern "C" fn(iter_ptr: *mut c_void) -> *mut c_char>,
    client_ls_free: libloading::Symbol<'lib, unsafe extern "C" fn(iter_ptr: *mut c_void)>,
    client_stat: libloading::Symbol<
        'lib,
        unsafe extern "C" fn(
            client: *const c_void,
            path: *const c_char,
            error: *mut c_char,
            error_size: c_uint,
        ) -> *mut c_char,
    >,
    client_rename: libloading::Symbol<
        'lib,
        unsafe extern "C" fn(
            client: *const c_void,
            src: *const c_char,
            dst: *const c_char,
        ) -> c_schar,
    >,
    client_rm: libloading::Symbol<
        'lib,
        unsafe extern "C" fn(client: *const c_void, path: *const c_char) -> c_schar,
    >,
    client_clear_trash: libloading::Symbol<
        'lib,
        unsafe extern "C" fn(client: *const c_void, path: *const c_char) -> c_schar,
    >,

    client_copy_file: libloading::Symbol<
        'lib,
        unsafe extern "C" fn(
            client: *const c_void,
            src: *const c_char,
            dst: *const c_char,
        ) -> c_schar,
    >,
    client_copy: libloading::Symbol<
        'lib,
        unsafe extern "C" fn(
            client: *const c_void,
            src: *const c_char,
            dst: *const c_char,
        ) -> c_schar,
    >,
}

fn client_connect(connect_fn: FnClientConnect) -> *const c_void {
    info!("Создаём клиент");
    let params = std::ffi::CString::new("").unwrap().into_raw();
    let mut error_ptr = [0; 1000];
    let client = unsafe { connect_fn(params, error_ptr.as_mut_ptr(), error_ptr.len() as c_uint) };
    let error_str = unsafe { std::ffi::CStr::from_ptr(error_ptr.as_ptr()) }.to_string_lossy();
    assert!(
        !client.is_null(),
        "Не удалось подключиться к серверу. {error_str}"
    );
    client
}

fn client_disconnect(disconnect_fn: FnClientDisconnect, client: *const c_void) {
    info!("Удаляем клиент");
    let count = unsafe { disconnect_fn(client) };
    assert_eq!(count, 0);
    info!("=== end ===");
}

#[test]
#[traced_test]
fn test_base_function() {
    info!("Загружаем библиотеку");
    let lib = DLibClient::new("../../target/debug/libfs4me_local.so");
    let DLibClientFn {
        connect,
        disconnect,
        client_cstring_free,
        client_info,
        client_time,
        ..
    } = lib.functions();

    let client = client_connect(connect);

    info!("Информация о драйвере");
    let info_ptr = unsafe { client_info(client) };
    let info_str = unsafe { std::ffi::CStr::from_ptr(info_ptr) }.to_string_lossy();
    debug!("info_str: {info_str}");
    assert!(
        info_str.starts_with("fs4me-local v"),
        "info_str: {info_str}"
    );
    info!("Освобождаем строку");
    unsafe { client_cstring_free(info_ptr) };

    info!("Время сервера");
    let time = unsafe { client_time(client) };
    debug!("time: {time}");

    client_disconnect(disconnect, client);
}

#[test]
#[traced_test]
fn test_mkdir() {
    let tmp_dir = tempdir().unwrap();
    let root_path = tmp_dir.path().canonicalize().unwrap();
    info!("Временная директория: {root_path:?}");

    info!("Загружаем библиотеку");
    let lib = DLibClient::new("../../target/debug/libfs4me_local.so");
    let DLibClientFn {
        connect,
        disconnect,
        client_exist,
        client_mkdir,
        ..
    } = lib.functions();

    let client = client_connect(connect);

    for dir_name in 0..3 {
        let dir_path = root_path.join(dir_name.to_string());

        info!("Создаём директории {dir_path:?}");
        let dir_cstr = std::ffi::CString::new(dir_path.to_string_lossy().into_owned())
            .unwrap()
            .into_raw();
        let result = unsafe { client_mkdir(client, dir_cstr, 0) };
        assert_eq!(result, 0, "Должно вернуться 0. Успешно создано");
        assert!(dir_path.exists(), "{dir_path:?} Должен существовать");

        info!("Проверяем существование директории {dir_path:?}");
        let exist = unsafe { client_exist(client, dir_cstr) };
        assert_eq!(exist, 1, "Должно вернуться 0. Директория существует");
    }

    info!("Рекурсивное создание");
    let dir_path = root_path.join("4/5/6");

    info!("Создаём директории {dir_path:?}");
    let dir_cstr = std::ffi::CString::new(dir_path.to_string_lossy().into_owned())
        .unwrap()
        .into_raw();
    let result = unsafe { client_mkdir(client, dir_cstr, 1) };
    assert_eq!(result, 0, "Должно вернуться 0. Успешно создано");
    assert!(dir_path.exists(), "{dir_path:?} Должен существовать");

    client_disconnect(disconnect, client);
}

#[test]
#[traced_test]
fn test_rw() {
    let tmp_dir = tempdir().unwrap();
    let root_path = tmp_dir.path().canonicalize().unwrap();
    info!("Временная директория: {root_path:?}");

    info!("Загружаем библиотеку");
    let lib = DLibClient::new("../../target/debug/libfs4me_local.so");
    let DLibClientFn {
        connect,
        disconnect,
        client_write_open,
        client_write_close,
        client_write,
        client_read,
        client_read_to_buffer,
        client_read_close,
        ..
    } = lib.functions();

    let client = client_connect(connect);

    info!("Создаём файл c флагом fail_if_exist");
    let file_path = root_path.join("a.txt");
    let file_path_cstr = std::ffi::CString::new(file_path.to_string_lossy().into_owned())
        .unwrap()
        .into_raw();
    let mode_cstr = std::ffi::CString::new("fail_if_exist".to_string())
        .unwrap()
        .into_raw();
    let mut error_ptr = [0; 1000];
    let writer = unsafe {
        client_write_open(
            client,
            file_path_cstr,
            mode_cstr,
            error_ptr.as_mut_ptr(),
            error_ptr.len() as c_uint,
        )
    };
    let error_str = unsafe { std::ffi::CStr::from_ptr(error_ptr.as_ptr()) }.to_string_lossy();
    assert!(!writer.is_null(), "Должно вернуться не null. {error_str}");

    for data in ["a", "b", "c"] {
        info!("Запись данных: {data}");
        let result = unsafe { client_write(writer, data.as_ptr(), data.len() as c_ulong) };
        assert_eq!(result, 0, "Должно вернуться 0. Успешно записано");
    }
    info!("Закрываем файл");
    let result = unsafe { client_write_close(writer) };
    assert_eq!(result as i8, 0, "Должно вернуться 0. Успешно закрыто");

    info!("Проверяем что файл был создан");
    assert!(file_path.exists(), "{file_path:?} Должен существовать");

    info!("Попытка повторного создания с флагом fail_if_exist");
    let mut error_ptr = [0; 1000];
    let writer = unsafe {
        client_write_open(
            client,
            file_path_cstr,
            mode_cstr,
            error_ptr.as_mut_ptr(),
            error_ptr.len() as c_uint,
        )
    };
    let error_str = unsafe { std::ffi::CStr::from_ptr(error_ptr.as_ptr()) }.to_string_lossy();
    info!("Ожидается ошибка: {error_str}");
    assert!(!error_str.is_empty(), "Должно вернуться не пустая ошибка.");
    assert!(
        writer.is_null(),
        "Ошибка: возвращен указатель на объект записи, хотя ожидалось отсутствие результата (флаг fail_if_exist должен был вернуть ошибку)"
    );

    info!("Сверяем содержимое файла");
    assert_eq!(
        fs::read_to_string(&file_path).unwrap(),
        "abc",
        "Содержимое не соответствует ожидаемому в {file_path:?}"
    );

    info!("Тестирование на чтение");
    let mut error = [0; 1000];
    let reader = unsafe {
        client_read(
            client,
            file_path_cstr,
            0,
            error.as_mut_ptr(),
            error.len() as c_uint,
        )
    };
    let error_str = unsafe { std::ffi::CStr::from_ptr(error.as_ptr()).to_string_lossy() };
    assert!(!reader.is_null(), "Должно вернуться не null. {error_str}");

    info!("Читаем содержимое файла");
    let mut buff = String::new();
    const BUF_SIZE: usize = 2;
    loop {
        let mut buf = [0u8; BUF_SIZE];
        let result = unsafe { client_read_to_buffer(reader, buf.as_mut_ptr(), BUF_SIZE as u32) };
        assert!(result >= 0, "Ошибка при чтении: {result}");
        if result == 0 {
            break;
        }
        // Преобразуем только прочитанные байты
        let data = &buf[..result as usize];
        buff.push_str(std::str::from_utf8(data).unwrap());
    }
    info!("Закрываем чтение");
    unsafe { client_read_close(reader) };
    info!("Сверяем прочитанное: {buff:?}");
    assert_eq!(&buff, "abc");

    info!("Тестирование режима записи append");
    let mode_cstr = std::ffi::CString::new("append").unwrap().into_raw();
    let mut error = [0; 1000];
    let writer = unsafe {
        client_write_open(
            client,
            file_path_cstr,
            mode_cstr,
            error.as_mut_ptr(),
            error.len() as c_uint,
        )
    };
    let error_str = unsafe { std::ffi::CStr::from_ptr(error.as_ptr()).to_string_lossy() };
    assert!(!writer.is_null(), "Ошибка: возвращен null. {error_str}");

    let data = b"z";

    let result = unsafe { client_write(writer, data.as_ptr(), data.len() as c_ulong) };
    assert_eq!(result, 0, "Ошибка при записи");

    info!("Закрываем запись");
    let result = unsafe { client_write_close(writer) };
    assert_eq!(result, 0, "Ошибка при закрытии файла");

    let content = fs::read_to_string(&file_path).unwrap();
    assert_eq!(content, "abcz");

    client_disconnect(disconnect, client);
}

#[test]
#[traced_test]
fn test_ls_stat() {
    let tmp_dir = tempdir().unwrap();
    let root_path = tmp_dir.path().canonicalize().unwrap();
    info!("Временная директория: {root_path:?}");

    info!("Создаем файлы и директории для тестирования");
    for file_name in ["a", "b", "c"] {
        let file_path = root_path.join(file_name).with_extension("txt");
        fs::write(&file_path, file_name).unwrap();
    }
    for dir_name in ["d", "e", "f"] {
        let dir_path = root_path.join(dir_name);
        fs::create_dir(&dir_path).unwrap();
    }

    info!("Загружаем библиотеку");
    let lib = DLibClient::new("../../target/debug/libfs4me_local.so");
    let DLibClientFn {
        connect,
        disconnect,
        client_ls,
        client_ls_next,
        client_ls_free,
        client_cstring_free,
        client_stat,
        ..
    } = lib.functions();

    let client = client_connect(connect);

    info!("Получаем ls итератор с вложенными файлами и директориями из {root_path:?}");
    let path_cstr_ptr = std::ffi::CString::new(root_path.to_string_lossy().into_owned())
        .unwrap()
        .into_raw();
    let mut error = [0; ERROR_LEN];
    let iter = unsafe {
        client_ls(
            client,
            path_cstr_ptr,
            error.as_mut_ptr(),
            ERROR_LEN as c_uint,
        )
    };
    let error_str = unsafe { std::ffi::CStr::from_ptr(error.as_ptr()).to_string_lossy() };
    assert!(
        !iter.is_null(),
        "client_ls вернул null итератор {error_str}",
    );

    let mut paths: Vec<String> = Vec::default();

    while let next = unsafe { client_ls_next(iter) }
        && !next.is_null()
    {
        let path = unsafe { std::ffi::CStr::from_ptr(next) }
            .to_string_lossy()
            .to_string();
        unsafe { client_cstring_free(next) };
        paths.push(path);
    }

    unsafe { client_ls_free(iter) };
    info!("Получены пути: {paths:#?}");

    let paths = paths.into_iter().map(PathBuf::from).collect::<Vec<_>>();
    assert!(
        paths.iter().all(|p| p.exists()),
        "Все пути должны существовать"
    );
    assert_eq!(paths.len(), 6, "Всего должно быть 6 путей");
    assert_eq!(
        paths.iter().filter(|p| p.is_file()).count(),
        3,
        "Всего должно быть 3 файла"
    );

    assert_eq!(
        paths.iter().filter(|p| p.is_dir()).count(),
        3,
        "Всего должно быть 3 директории"
    );

    info!("Тестируем получение информации о пути");
    for file_path in &paths {
        let file_path_str = file_path.to_str().unwrap();
        let paths_cstr = CString::new(file_path_str).unwrap();
        let path_cstr_ptr = paths_cstr.into_raw();
        let mut error = [0; ERROR_LEN];
        let stat_cstr_ptr = unsafe {
            client_stat(
                client,
                path_cstr_ptr,
                error.as_mut_ptr(),
                ERROR_LEN as c_uint,
            )
        };
        let error_str = unsafe { std::ffi::CStr::from_ptr(error.as_ptr()).to_string_lossy() };
        assert!(
            !stat_cstr_ptr.is_null(),
            "stat_cstr_ptr не должен быть null. {error_str}"
        );
        let stat = unsafe { std::ffi::CStr::from_ptr(stat_cstr_ptr).to_string_lossy() };
        info!("stat: {stat}");

        if file_path.is_dir() {
            assert!(stat.starts_with("dir"));
        } else {
            assert!(stat.starts_with("file"));
        }

        unsafe { client_cstring_free(stat_cstr_ptr) };
    }

    client_disconnect(disconnect, client);
}

#[test]
#[traced_test]
fn test_rename() {
    let tmp_dir = tempdir().unwrap();
    let root_path = tmp_dir.path().canonicalize().unwrap();
    info!("Временная директория: {root_path:?}");

    let src_dir = root_path.join("src");
    info!(?src_dir, "Создаём директорию для перемещения");
    fs::create_dir(&src_dir).unwrap();

    let dst_dir = root_path.join("dst");

    info!("Загружаем библиотеку");
    let lib = DLibClient::new("../../target/debug/libfs4me_local.so");
    let DLibClientFn {
        connect,
        disconnect,
        client_rename,
        ..
    } = lib.functions();

    let client = client_connect(connect);

    let src_path_str = CString::new(src_dir.to_string_lossy().to_string())
        .unwrap()
        .into_raw();
    let dst_path_str = CString::new(dst_dir.to_string_lossy().to_string())
        .unwrap()
        .into_raw();
    info!(?src_dir, ?dst_dir, "Перемещаем");
    let result = unsafe { client_rename(client, src_path_str, dst_path_str) };
    assert_eq!(result, 0);

    assert!(dst_dir.exists());
    assert!(!src_dir.exists());

    client_disconnect(disconnect, client);
}

#[test]
#[traced_test]
fn test_rm() {
    let tmp_dir = tempdir().unwrap();
    let root_path = tmp_dir.path().canonicalize().unwrap();
    info!("Временная директория: {root_path:?}");

    let dir_path = root_path.join("src");
    info!(?dir_path, "Создаём директорию для удаления");
    fs::create_dir(&dir_path).unwrap();

    info!("Загружаем библиотеку");
    let lib = DLibClient::new("../../target/debug/libfs4me_local.so");
    let DLibClientFn {
        connect,
        disconnect,
        client_rm,
        client_clear_trash,
        ..
    } = lib.functions();

    let client = client_connect(connect);

    let dir_path_ptr = CString::new(dir_path.to_string_lossy().to_string())
        .unwrap()
        .into_raw();
    info!(?dir_path, "Удаляем");
    let result = unsafe { client_rm(client, dir_path_ptr) };
    assert_eq!(result, 0);
    assert!(!dir_path.exists());

    info!("Проверяем, что директория перемещена в корзину");
    let new_dir_path = root_path.join(".trash/src");
    assert!(new_dir_path.exists());

    info!("Очищаем корзину");
    let root_cstr_ptr = CString::new(root_path.to_string_lossy().to_string())
        .unwrap()
        .into_raw();
    unsafe { client_clear_trash(client, root_cstr_ptr) };
    assert!(!new_dir_path.exists());
    info!("Корзина очищена");

    client_disconnect(disconnect, client);
}

#[test]
#[traced_test]
fn test_cp() {
    let tmp_dir = tempdir().unwrap();
    let root_path = tmp_dir.path().canonicalize().unwrap();
    info!("Временная директория: {root_path:?}");

    let from_dir = root_path.join("src");
    info!(?from_dir, "Создаём директорию для копирования");
    fs::create_dir(&from_dir).unwrap();

    let file_path = from_dir.join("test.txt");
    info!(?file_path, "Создаём файл для копирования");
    fs::write(&file_path, "test").unwrap();

    info!("Загружаем библиотеку");
    let lib = DLibClient::new("../../target/debug/libfs4me_local.so");
    let DLibClientFn {
        connect,
        disconnect,
        client_copy_file,
        client_copy,
        ..
    } = lib.functions();

    let client = client_connect(connect);

    let to_dir = root_path.join("dst");
    info!(?from_dir, ?to_dir, "Рекурсивное копирование директории");
    let from_ptr = CString::new(from_dir.to_string_lossy().to_string())
        .unwrap()
        .into_raw();
    let to_ptr = CString::new(to_dir.to_string_lossy().to_string())
        .unwrap()
        .into_raw();
    let result = unsafe { client_copy(client, from_ptr, to_ptr) };
    assert_eq!(result, 0);
    assert!(from_dir.exists());
    assert!(to_dir.exists());
    let to_file_path = to_dir.join("test.txt");
    assert!(to_file_path.exists());

    let from_file = to_file_path;
    let to_file = to_dir.join("new.txt");
    info!(?from_file, ?to_file, "Копирование файла");
    let from_ptr = CString::new(from_file.to_string_lossy().to_string())
        .unwrap()
        .into_raw();
    let to_ptr = CString::new(to_file.to_string_lossy().to_string())
        .unwrap()
        .into_raw();
    let result = unsafe { client_copy_file(client, from_ptr, to_ptr) };
    assert_eq!(result, 0);
    assert!(from_file.exists());
    assert!(to_file.exists());

    client_disconnect(disconnect, client);
}
