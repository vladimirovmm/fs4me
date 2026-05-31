# fs4me — Рабочая файловая система на Rust

**fs4me** — это Rust-пакет с локальным файловым драйвером, который обеспечивает безопасный одновременный доступ к файлам через lock-механизмы, корзину (trash) и UUID-идентификаторы.

## 🎯 Возможности

- ✅ Безопасный одновременный доступ к файлам (read/write locks)
- ✅ Корзина (trash) для безопасного удаления файлов и директорий
- ✅ UUID-идентификаторы для отслеживания соединений
- ✅ Рекурсивное создание и удаление директорий
- ✅ Поддержка режимов записи: overwrite, append, fail-if-exists
- ✅ Интеграция с C-программами (FFI)
- ✅ F-тесты для тестирования файловых систем

## 📦 Пакеты

- **`fs4me-interface`** — базовый интерфейс с trait `Driver` и параметрами подключения
- **`fs4me-local`** — локальный файловый драйвер для работы с файловой системой
- **`fs4me-client`** — обёртка над драйвером с lock-механизмами, корзиной и UUID

## 🚀 Быстрый старт

### 1. Установка в `Cargo.toml`

```toml
[dependencies]
fs4me-local = { path = "./driver/local" }
fs4me-client = { path = "./client" }
```

### 2. Пример использования

```rust
use fs4me_local::LocalDriver;
use fs4me_interface::DriverParams;

fn main() {
    // Подключение к локальной файловой системе
    let driver = LocalDriver::connect(DriverParams::default())
        .unwrap();

    // Обёртка с lock-механизмами и UUID
    let client = fs4me_client::Fs::new(driver);

    // Перечисление директории
    let files: Vec<_> = client.ls("/")
        .unwrap()
        .filter_map(|p| p.to_str())
        .collect();

    // Получение информации о драйвере
    println!("Драйвер: {}", client.driver_info());

    // Текущее время сервера
    if let Ok(time) = client.time() {
        println!("Время сервера: {}", time);
    }
}
```

## 📁 Структура проекта

```
fs4me/
├── Cargo.toml                      # Workspace конфигурация
├── README.md                       # Этот файл
├── SPECIFICATION.md                # Детальная спецификация
├── codebook.toml                   # Конфигурация проекта
├── driver/
│   └── local/                      # Локальный драйвер
│       └── src/
│           ├── lib.rs              # Реализация локального драйвера
│           ├── interface/          # Trait Driver и вспомогательные типы
│           │   ├── mod.rs          # Основной модуль
│           │   ├── open_params.rs  # Параметры подключения
│           │   └── errors.rs       # Обработка ошибок
│           ├── open/               # Открытие файлов
│           │   ├── mod.rs          # Основной модуль
│           │   ├── read.rs         # Чтение файлов
│           │   └── write.rs        # Запись файлов
│           ├── mkdir/              # Создание директорий
│           │   └── mod.rs
│           ├── stat/               # Получение статистики
│           │   └── mod.rs
│           ├── mv/                 # Перемещение файлов
│           │   └── mod.rs
│           ├── rm/                 # Удаление файлов
│           │   └── mod.rs
│           └── ffi/                # FFI-экспорт
│               ├── mod.rs
│               ├── connect.rs
│               ├── info.rs
│               ├── read.rs
│               └── write.rs
├── interface/
│   └── src/
│       ├── lib.rs                  # Trait `Driver` и вспомогательные типы
│       ├── open_params.rs          # Параметры подключения
│       └── errors.rs               # Обработка ошибок
├── client/
│   └── src/
│       ├── lib.rs                  # Обёртка с lock-механизмами
│       ├── lock.rs                 # Реализация блокировок
│       ├── trash.rs                # Корзина (trash)
│       └── uuid.rs                 # UUID-идентификаторы
├── target/                         # Build-артефакты
└── .gitignore                      # Игнорируемые файлы
```

## 🏗️ Интерфейс (Trait `Driver`)

Trait `Driver` определяет стандартный набор методов для работы с файлами:

| Метод | Описание |
|-------|----------|
| `name()` | Возвращает название драйвера |
| `version()` | Возвращает версию драйвера |
| `info()` | Возвращает строку "имя + версия" |
| `connect()` | Подключение к хранилищу с параметрами |
| `disconnect()` | Отключение и очистка ресурсов |
| `server_time()` | Текущее время сервера (Unix timestamp) |
| `exists()` | Проверка существования файла/директории |
| `ls()` | Перечисление содержимого директории |
| `stat()` | Получение информации о файле/директории |
| `mv()` | Перемещение/переименование файла |
| `mkdir()` | Создание директории (рекурсивно) |
| `rm()` | Удаление файла/директории |
| `read()` | Чтение файла с указательной позицией |
| `write()` | Запись в файл с режимом (append/overwrite/fail) |

### Параметры подключения

```rust
use fs4me_interface::DriverParams;

// Из HashMap
let params = DriverParams::from([
    ("PATH".to_string(), "/some/path".to_string()),
    ("VERBOSE".to_string(), "true".to_string()),
]);

// Из строки (формат KEY=VALUE\nKEY=VALUE)
let params = DriverParams::from("PATH=/some/path\nVERBOSE=true");

// Из C-строки
let params = DriverParams::from(CString::new("PATH=/some/path").unwrap());
```

## 🔒 Безопасность и lock-механизмы

Все операции чтения и записи защищены блокировками:

- ✅ **Заблокированные операции**: `read`, `write`, `mv`, `rm`
- ✅ **Автоматическое освобождение**: блокировки освобождаются по выходу из области видимости
- ✅ **Рекурсивные блокировки**: родительская директория блокируется при операции

```rust
// Чтение файла с автоматическим lock-ом
let reader = client.read("/path/to/file.txt", 0)?;
// ... чтение ...
// Lock автоматически освобождается при выходе из области видимости

// Запись в файл с автоматическим lock-ом
let writer = client.write("/path/to/file.txt", WriteMode::Overwrite)?;
// ... запись ...
// Lock автоматически освобождается при выходе из области видимости
```

## 🗑️ Корзина (Trash)

Метод `rm()` перемещает файл/директорию в корзину, а не удаляет его безвозвратно:

```rust
// Перемещение файла в корзину
client.rm("/path/to/file.txt")?;

// Файл перемещён в уникальное место (через `trash_unique_path`)
// Чтобы восстановить файл, нужно переместить его обратно:
client.mv("/trash/path/to/file.txt", "/path/to/file.txt")?;
```

## 📊 Зависимости

### Основные
- `thiserror` — обработка ошибок
- `rand` — генерация UUID
- `chrono` — работа с временными метками
- `tracing` — логирование
- `tempfile` — тестирование

### Dev-зависимости
- `tracing-test` — тестирование логирования
- `tracing-subscriber` — JSON-логирование

## 🔬 Тестирование

```bash
# Unit-тесты
cargo test

# Просмотр логов тестов
cargo test -- --nocapture
```

Существующие тесты:
- `test_driver_info` — проверка имени и версии
- `test_time` — проверка времени сервера
- `test_ls` — перечисление директории
- `test_rename` — перемещение файлов
- `test_work_with_directory` — комплексные тесты
- `test_rw` — тесты чтения/записи

## 🧩 Типы и структуры

### `Stat` — Информация о файле/директории

```rust
pub struct Stat {
    pub name: String,              // Имя файла/директории
    pub path: PathBuf,             // Полный путь
    pub kind: FileKind,            // Тип: File, Dir, Link
    pub size: u64,                 // Размер (для файлов)
    pub modified: DateTime<Utc>,   // Время изменения
    pub created: DateTime<Utc>,    // Дата создания
    pub deleted: Option<DateTime<Utc>>, // Дата удаления
}
```

### `DriverParams` — Параметры подключения

```rust
pub struct DriverParams {
    pub params: HashMap<String, String>,
}

// Создание из HashMap
let params = DriverParams::from([
    ("PATH".to_string(), "/some/path".to_string()),
    ("VERBOSE".to_string(), "true".to_string()),
]);

// Создание из строки
let params = DriverParams::from("PATH=/some/path\nVERBOSE=true");

// Создание из C-строки
let params = DriverParams::from(CString::new("PATH=/some/path").unwrap());
```

### `WriteMode` — Режимы записи

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteMode {
    Overwrite,   // Записать с перезаписью существующих данных
    Append,      // Записать в конец файла
    FailIfExists, // Не создавать файл если он уже существует
}
```

### `LockMode` — Режимы блокировок

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockMode {
    Read,    // Блокировка для чтения
    Write,   // Блокировка для записи
}
```

### `FileKind` — Типы файлов

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    File,     // Обычный файл
    Dir,      // Директория
    Link,     // Символическая ссылка
}
```

### `Read` — Чтение файла

```rust
pub struct Read<'a, S: Driver>
where
    S::Open<'a>: Read,
{
    // Чтение с указательной позицией
    fn read_to_string(&mut self) -> Result<String, S::Error>;
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, S::Error>;
    fn seek(&mut self, pos: SeekPosition) -> Result<u64, S::Error>;
}

pub enum SeekPosition {
    Start(u64),   // От начала файла
    Current(i64), // От текущей позиции
    End(i64),     // От конца файла
}
```

### `Write` — Запись в файл

```rust
pub struct Write<'a, S: Driver>
where
    S::Open<'a>: Write,
{
    // Запись данных
    fn write_all(&mut self, data: &[u8]) -> Result<(), S::Error>;
    fn write(&mut self, data: &[u8]) -> Result<usize, S::Error>;
    fn flush(&mut self) -> Result<(), S::Error>;
}
```

## 📦 Использование пакетов

### `fs4me-interface` (общий интерфейс)

```toml
[dependencies]
fs4me-interface = { path = "../interface" }
```

Пример:

```rust
use fs4me_interface::Driver;

trait Driver {
    type Open<'a>: Send + Sync;
    type Error: std::error::Error;

    fn name(&self) -> &str;
    fn version(&self) -> &str;
    fn info(&self) -> String;
    fn connect(params: DriverParams) -> Result<Self, Self::Error> where Self: Sized;
    fn disconnect(&mut self);
}
```

### `fs4me-local` (локальный драйвер)

```toml
[dependencies]
fs4me-local = { path = "../driver/local" }
```

Пример:

```rust
use fs4me_local::LocalDriver;
use fs4me_interface::DriverParams;

// Подключение к локальной файловой системе
let driver = LocalDriver::connect(DriverParams::default())
    .unwrap();

// Получение информации о драйвере
println!("Драйвер: {}", driver.info());
```

### `fs4me-client` (обёртка с lock-механизмами)

```toml
[dependencies]
fs4me-client = { path = "../client" }
```

Пример:

```rust
use fs4me_client::Fs;
use fs4me_local::LocalDriver;
use fs4me_interface::DriverParams;

fn main() -> anyhow::Result<()> {
    // Подключение
    let driver = LocalDriver::connect(DriverParams::default())?;

    // Создание обёртки
    let client = Fs::new(driver);

    // Перечисление директории
    let files: Vec<_> = client.ls("/")
        .unwrap()
        .filter_map(|p| p.to_str())
        .collect();

    // Создание директории
    client.mkdir("/new_dir", true)?;

    // Перемещение файла
    client.mv("/old_file.txt", "/new_file.txt")?;

    // Перемещение в корзину
    client.rm("/to_delete.txt")?;

    // Чтение файла
    let reader = client.read("/data.txt", 0)?;
    let data = reader.read_to_string()?;
    println!("Контент: {}", data);

    // Запись в файл
    let writer = client.write("/output.txt", WriteMode::Overwrite)?;
    writer.write_all(b"Hello, world!")?;

    Ok(())
}
```

## 🔧 FFI-экспорт

Модуль `driver/local/src/ffi` предоставляет обёртки для взаимодействия с файловой системой из C-программ через `extern "C"`:

### Доступные функции

#### `fs4me_connect`

```c
#include "fs4me.h"

// Подключение к файловой системе
Fs* fs = fs4me_connect("PATH=/some/path", true);

// Отключение
fs4me_disconnect(fs);
```

#### `fs4me_info`

```c
// Получение строки "имя + версия"
const char* info = fs4me_info(fs);
fs4me_free_string(info);
```

#### `fs4me_read`

```c
// Чтение файла
char* data = fs4me_read(fs, "/path/to/file.txt", 0);
int len = fs4me_free_string(data);  // Возвращает длину строки
```

#### `fs4me_write`

```c
// Запись в файл
bool result = fs4me_write(fs, "/path/to/file.txt", "/some/data.txt", true);
```

### Использование в C-проекте

```c
#include "fs4me.h"

int main() {
    // Подключение
    Fs* fs = fs4me_connect("PATH=/home/user", true);
    
    if (!fs) {
        fprintf(stderr, "Ошибка подключения\n");
        return 1;
    }

    // Чтение файла
    char* content = fs4me_read(fs, "/etc/passwd", 0);
    if (content) {
        printf("Первые 100 символов: %.100s\n", content);
        fs4me_free_string(content);
    }

    // Запись в файл
    fs4me_write(fs, "/tmp/output.txt", "Hello from C!\n", true);
    
    // Отключение
    fs4me_disconnect(fs);
    
    return 0;
}
```

## 🔧 Модуль `ffi_tests`

Модуль `driver/local/src/ffi_tests` содержит тесты для проверки FFI-функций:

```c
#include "fs4me.h"
#include <stdio.h>

int main() {
    // Тест подключения
    Fs* fs = fs4me_connect("PATH=/tmp", true);
    if (!fs) {
        fprintf(stderr, "Ошибка подключения\n");
        return 1;
    }

    printf("Драйвер: %s\n", fs4me_info(fs));

    // Тест перечисления директории
    char** files = fs4me_ls(fs, "/");
    if (files) {
        for (int i = 0; files[i] != NULL; i++) {
            printf("Файл: %s\n", files[i]);
            fs4me_free_string(files[i]);
        }
        fs4me_free_string_array(files);
    }

    // Тест чтения файла
    char* content = fs4me_read(fs, "/etc/hostname", 0);
    if (content) {
        printf("hostname: %.100s\n", content);
        fs4me_free_string(content);
    }

    // Тест записи файла
    fs4me_write(fs, "/tmp/test_ffi.txt", "Привет, FFI!\n", true);
    if (fs4me_exists(fs, "/tmp/test_ffi.txt")) {
        printf("Файл успешно создан\n");
    }

    fs4me_disconnect(fs);
    return 0;
}
```

## 🚀 План развития

1. Реализовать модуль `ffi_tests`
2. Добавить поддержку `SFTP`, `FTP`, `WebDav` и других файловых систем
3. Реализовать общий макрос для FFI-экспорта
4. Полная документация для разработчиков
5. Добавить поддержку потоков и асинхронных операций

## 📄 Лицензия

Проект распространяется по лицензии MIT.

## 📞 Контакты

**Автор:** VMM <vladimirov.m.m@mail.ru>

**GitHub:** [fs4me](https://github.com/vladimirovmm/fs4me)

**Вопросы?** — Создайте Issue или отправьте Pull Request!
