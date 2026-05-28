# Спецификация проекта fs4me

## Обзор

fs4me — Rust workspace с драйвером для работы с локальными файловыми системами. Проект обеспечивает унифицированный интерфейс для операций с файлами и директориями, с поддержкой FFI-экспорта для интеграции с C-программами.

**Версия:** 0.2.0
**Язык:** Rust 2024 edition
**Тип:** Workspace с чейн-тип `cdylib`

---

## Архитектура

Проект состоит из следующих компонентов:

```
fs4me/
├── Cargo.toml              # Workspace конфигурация
├── README.md               # Этот файл
├── SPECIFICATION.md        # Детальная спецификация
├── driver/
│   └── local/              # Локальный драйвер
│       └── src/
│           ├── lib.rs      # Основной модуль
│           ├── interface/  # Trait Driver и вспомогательные структуры
│           │   ├── mod.rs  # Основной модуль
│           │   ├── open_params.rs
│           │   └── writer.rs
│           ├── ffi/        # FFI-экспорт
│           │   ├── mod.rs  # Основной модуль
│           │   ├── dir.rs
│           │   └── ls.rs
│           ├── file.rs     # Работа с файлами
│           └── rw.rs       # Чтение/запись
├── ffi_tests/              # Тесты FFI (будущая реализация)
└── target/                 # Build-артефакты
```

---

## Компоненты

### Driver (Черта)

`driver/local/src/interface/mod.rs` определяет `trait Driver`, обеспечивающий унифицированный интерфейс для всех драйверов.

#### Основные методы

| Метод | Подпись | Описание |
|-------|---------|----------|
| `name()` | `fn name(&self) -> &str` | Имя драйвера |
| `version()` | `fn version(&self) -> &str` | Версия драйвера |
| `info()` | `fn info(&self) -> String` | Информация о драйвере (имя, версия) |
| `connect()` | `fn connect<P: Into<DriverParams>>(&self, P) -> Result<Self>` | Подключение к хранилищу |
| `disconnect()` | `fn disconnect(&self) -> Result<()>` | Отключение и очистка ресурсов |
| `server_time()` | `fn server_time(&self) -> Result<u32>` | Текущее время сервера (UNIX timestamp) |
| `ls()` | `fn ls<P: AsRef<Path>>(&self, P) -> Result<impl Iterator<Item = PathBuf>>` | Перечисление содержимого директории |
| `exists()` | `fn exists<P: AsRef<Path>>(&self, P) -> bool` | Проверка существования файла/директории |
| `rename()` | `fn rename<P: AsRef<Path>>(&self, P, P) -> Result<()>` | Перемещение файла/директории |
| `mkdir()` | `fn mkdir<P: AsRef<Path>>(&self, P, recursive: bool) -> Result<()>` | Создание директории |
| `rm()` | `fn rm<P: AsRef<Path>>(&self, P) -> Result<()>` | Удаление файла/директории в корзину |
| `stat()` | `fn stat<P: AsRef<Path>>(&self, P) -> Result<Stat>` | Получение информации о файле/директории |
| `unsafe_write()` | `fn unsafe_write<P: AsRef<Path>>(&self, P, mode: WriteMode) -> Result<Box<dyn io::Write>>` | Безопасное открытие файла для записи |
| `write()` | `fn write<P: AsRef<Path>>(&self, P, mode: WriteMode) -> Result<DriverWriter<Self>>` | Запись в файл с блокировкой |
| `read()` | `fn read<P: AsRef<Path>>(&self, P, position: u64) -> Result<Box<dyn io::Read>>` | Чтение файла с позицией |
| `disconnect()` | `fn disconnect(&self) -> Result<()>` | Отключение от хранилища |

#### DriverParams

Структура для параметров подключения, поддерживает несколько форматов:
- `HashMap<String, String>`
- `&str` (строка с форматом `KEY=VALUE\nKEY=VALUE`)
- `CString` (C-строка)
- `String`

Формат C-строки: ключевые пары разделены символами `\n`. Можно передать как `CString`, `&CStr`, `str`, `String`.

---

### FFI (C-like API)

Модуль `driver/local/src/ffi/mod.rs` экспортирует функции для взаимодействия на уровне C:

| Функция | Подпись | Описание |
|-------|--|------|
| `client_connect()` | `pub unsafe extern "C" fn client_connect(params_ptr: *const c_char) -> *mut c_void` | Подключение к хранилищу. Возвращает указатель на `LocalDriver` (Box) или NULL |
| `client_disconnect()` | `pub unsafe extern "C" fn client_disconnect(client_ptr: *mut c_void)` | Отключение от хранилища. Освобождает ресурсы. |
| `client_get_info()` | `pub unsafe extern "C" fn client_get_info(client_ptr: *mut c_void) -> *mut c_char` | Получение информации о драйвере. Возвращает указатель на C-строку |
| `free_c_string()` | `pub unsafe extern "C" fn free_c_string(ptr: *mut c_char)` | Освобождение C-строки, возвращённой `client_get_info()` |
| `client_server_time()` | `pub unsafe extern "C" fn client_server_time(client_ptr: *mut c_void) -> u32` | Текущее время сервера в UNIX-времени |
| `client_exists()` | `pub unsafe extern "C" fn client_exists(client_ptr: *mut c_void, path: *const c_char) -> bool` | Проверка существования файла/директории |
| `client_mkdir()` | `pub unsafe extern "C" fn client_mkdir(client_ptr: *mut c_void, path: *const c_char, recursive: bool) -> i32` | Создание директории. Возвращает код ошибки |
| `client_rmdir()` | `pub unsafe extern "C" fn client_rmdir(client_ptr: *mut c_void, path: *const c_char) -> i32` | Удаление файла/директории в корзину. Возвращает код ошибки |

| Функция | Подпись | Описание |
|-------|--|------|
| `client_ls()` | `pub unsafe extern "C" fn client_ls(client_ptr: *mut c_void, path_ptr: *const c_char) -> *mut LsIteratorHandle` | Открытие итератора для директории |
| `client_ls_next()` | `pub unsafe extern "C" fn client_ls_next(handle: *mut LsIteratorHandle) -> *mut c_char` | Получить следующую строку из итератора. Возвращает указатель на C-строку (имя файла/директории) или NULL |
| `client_ls_has_next()` | `pub unsafe extern "C" fn client_ls_has_next(handle: *mut LsIteratorHandle) -> bool` | Проверка, остались ли элементы в итераторе |
| `client_ls_free()` | `pub unsafe extern "C" fn client_ls_free(handle: *mut LsIteratorHandle)` | Освобождение итератора |

**Коды ошибок в `client_mkdir()` и `client_rmdir()`:**
- `0` - успех
- `1` - ошибка преобразования указателя `client_ptr`
- `2` - ошибка преобразования указателя `path`
- `3` - внутренняя ошибка драйвера

---

### Файлы (Реализация драйвера)

Модули `driver/local/src/file.rs` и `driver/local/src/rw.rs` работают с файлами:

| Функция | Описание |
|--------|--------|
| `open_file()` | Безопасное открытие файла с поддержкой блокировок `.lock` |
| `write_file()` | Запись с автоматическим освобождением ресурсов |
| `read_file()` | Чтение с позицией |

**Пример безопасной записи:**
```rust
fn safe_write(path: &Path, data: &[u8]) -> Result<()> {
    let driver = LocalDriver::connect(HashMap::new())?;
    driver.write(path, data)?;
    Ok(())
}
```

---

### Тесты

Тесты драйвера расположены в `driver/local/src/lib.rs` (модуль `tests`):

| Тест | Описание |
|-------|--|
| `test_driver_info` | Проверка получения информации о драйвере |
| `test_time` | Проверка времени сервера |
| `test_ls` | Проверка перечисления директории |
| `test_rename` | Проверка перемещения файлов |
| `test_work_with_directory` | Проверка работы с директориями (mkdir, rmdir) |
| `test_write` | Проверка чтения и записи файлов |

---

### FFI-тесты

`driver/local/src/lib.rs` реализует локальный драйвер для работы с файловой системой:

#### Функциональность

- **`connect()`** — подключается к локальной файловой системе (по умолчанию использует текущий пользователь)
- **`disconnect()`** — освобождает ресурсы (вызывается в `Drop`)
- **`ls()`** — возвращает интератор путей файлов и директорий
- **`mkdir()`** — создаёт директорию с поддержкой рекурсивного создания
- **`rm()`** — удаляет файлы и директории в "корзину". При удалении непустой директории сначала переименовывает её в скрытую, чтобы избежать конфликтов. Поддерживает рекурсивное удаление
- **`stat()`** — получает информацию о файле/директории (размер, время, режим)
- **`rename()`** — перемещает файл/директорию с именем
- **`exists()`** — проверяет существование файла или директории
- **`server_time()`** — возвращает текущее время UNIX timestamp
- **`info()`** — возвращает строку с информацией: `name=LocalDriver\nversion=<версия>`
- **`unsafe_write()`** — безопасно открывает файл для записи (возвращает `Box<dyn io::Write>`)
- **`write()`** — запись в файл с блокировкой (возвращает `DriverWriter<Self>`)
- **`read()`** — чтение файла с позицией (возвращает `Box<dyn io::Read>`)
- **`read()`** — читает содержимое файла в вектор байтов
- **`write()`** — записывает данные в файл
- **`append()`** — дописывает данные в конец файла
- **`delete()`** — удаляет файл
- **`rename()`** — переименовывает или перемещает файл

#### Безопасность

- Удаление непустых директорий безопасно — используется механизм переименования
- Файловые операции содержат проверки на доступность пути
- FFI-функции помечены как `unsafe`, но содержат проверки на NULL
- `client_disconnect` освобождает память через `Box::from_raw`

---

### FFI API (C-подобный интерфейс)

Модуль `driver/local/src/ffi/mod.rs` экспортирует функции C-подобного интерфейса:

| Функция | Подпись | Описание |
|---------|---------|----------|
| `client_connect` | `extern "C" fn client_connect(params_ptr: *const c_char) -> *mut c_void` | Подключение к файловому хранилищу |
| `client_disconnect` | `extern "C" fn client_disconnect(client_ptr: *mut c_void)` | Отключение и освобождение ресурсов |
| `client_get_info` | `extern "C" fn client_get_info(client_ptr: *mut c_void) -> *const c_char` | Получение информации о драйвере |
| `client_server_time` | `extern "C" fn client_server_time(client_ptr: *mut c_void) -> u32` | Текущее время сервера |
| `client_exists` | `extern "C" fn client_exists(client_ptr: *mut c_void, path_ptr: *const c_char) -> bool` | Проверка существования файла |
| `client_mkdir` | `extern "C" fn client_mkdir(client_ptr: *mut c_void, path_ptr: *const c_char, recursive: bool) -> i32` | Создание директории |
| `client_rmdir` | `extern "C" fn client_rmdir(client_ptr: *mut c_void, path_ptr: *const c_char) -> i32` | Удаление файла/директории |
| `client_ls` | `extern "C" fn client_ls(client_ptr: *mut c_void, path_ptr: *const c_char) -> *mut LsIteratorHandle` | Открытие итератора |
#### Безопасность FFI

- Все FFI-функции помечены как `unsafe`, но содержат внутренние проверки на NULL
- Память возвращается через `Box::from_raw` для `client_disconnect`
- C-строки возвращаются через `String::into_string()` с последующим `CString::from_string_with_nul()`
- Коды ошибок в `client_mkdir` и `client_rmdir`: 0 (OK), 1 (invalid client), 2 (invalid path), 3 (internal error)

- Все функции помечены `#[unsafe(no_mangle)]`
- Проверки на NULL возвращают соответствующие значения ошибок
- `client_ls` использует `Box::leak` для освобождения итератора в `client_ls_free` (невозвращаемый ресурс)

---

## Формат параметров подключения

Параметры передаются в формате `KEY=VALUE\nKEY=VALUE`:

```
PATH=/some/path
TIMEOUT=30
VERBOSE=true
FILES_ENABLED=true
READ_ENABLED=true
WRITE_ENABLED=true
```

Допустимые пустые строки и строки с пробелами игнорируются. Флаги `FILES_ENABLED`, `READ_ENABLED`, `WRITE_ENABLED` позволяют включать/отключать конкретные операции.

---

## Ошибки

Используется библиотека `eyre` для управления ошибками:
- Результат операций: `Result<T, eyre::Report>`
- Ошибки содержат контекст (функция, путь, детали)

---

## Тестирование

### Unit-тесты

Тесты расположены в `driver/local/src/lib.rs` (модуль `tests`):

| Тест | Описание |
|-------|--|
| `test_driver_info` — проверка имени и версии |
| `test_time` — проверка времени сервера |
| `test_ls` — перечисление директории |
| `test_rename` — перемещение файлов |
| `test_work_with_directory` — комплексный тест директорий (создание, удаление, перечисление) |
| `test_write` — тесты операций с файлами (чтение, запись) |

### FFI-тесты

Модуль `ffi_tests` (ещё не реализован) предназначен для тестирования C-подобного интерфейса.

---

## Зависимости

### Основные

- `eyre` — обработка ошибок
- **`rand`** — генерация случайных чисел
- **`std::fs`, `std::path`** — работа с файловой системой

### Dev-зависимости

- `tracing` — логирование
- `tracing-subscriber` — JSON-логирование
- `tempfile` — временные директории

---

## Версии

- **name:** `LocalDriver`
- **version:** `0.1.0` (берётся из `Cargo.toml`)

---

## План развития

1. Реализовать `ffi_tests`
2. Добавить поддержку файловых операций (чтение, запись)
3. Реализовать общий модуль FFI через макросы
4. Поддержка других файловых систем (например, `memfs`)
5. Добавить документацию для разработчиков
