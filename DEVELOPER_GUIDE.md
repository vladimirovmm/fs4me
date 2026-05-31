# Файловый драйвер на Rust: Документация для разработчиков

## 🎯 Обзор проекта

fs4me — это Rust-пакет для работы с локальными файловой системой с унифицированным интерфейсом `Driver`. Пакет обеспечивает безопасный одновременный доступ к файлам через lock-механизмы, корзину (trash) и UUID-идентификаторы.

### Структура проекта

```
fs4me/
├── Cargo.toml                        # Workspace конфигурация
├── README.md                         # Основной README
├── DEVELOPER_GUIDE.md                # Эта документация
├── SPECIFICATION.md                  # Детальная спецификация
├── driver/
│   └── local/                        # Локальный драйвер
│       └── src/
│           ├── lib.rs                # Основной модуль драйвера
│           ├── interface/            # Trait Driver и вспомогательные типы
│           │   ├── mod.rs            # Основной модуль
│           │   ├── open_params.rs    # Параметры подключения
│           │   └── errors.rs         # Обработка ошибок
│           ├── open/                 # Открытие файлов
│           │   ├── mod.rs            # Основной модуль
│           │   ├── read.rs           # Чтение файлов
│           │   └── write.rs          # Запись файлов
│           ├── mkdir/                # Создание директорий
│           │   └── mod.rs
│           ├── stat/                 # Получение статистики
│           │   └── mod.rs
│           ├── mv/                   # Перемещение файлов
│           │   └── mod.rs
│           ├── rm/                   # Удаление файлов
│           │   └── mod.rs
│           └── ffi/                  # FFI-экспорт
│               ├── mod.rs
│               ├── connect.rs
│               ├── info.rs
│               ├── read.rs
│               └── write.rs
├── interface/
│   └── src/
│       ├── lib.rs                    # Trait Driver и вспомогательные типы
│       ├── open_params.rs            # Параметры подключения
│       └── errors.rs                 # Обработка ошибок
├── client/
│   └── src/
│       ├── lib.rs                    # Обёртка с lock-механизмами
│       ├── lock.rs                   # Реализация блокировок
│       ├── trash.rs                  # Корзина (trash)
│       └── uuid.rs                   # UUID-идентификаторы
└── target/                           # Build-артефакты
```

## 🏗️ Архитектура

### Trait `Driver`

Интерфейс для работы с файловыми системами. Все драйверы должны реализовать этот trait.

#### Основные методы

| Метод | Возврат | Описание |
|-------|---------|----------|
| `name()` | `&'static str` | Имя драйвера |
| `version()` | `&'static str` | Версия драйвера |
| `info()` | `String` | Строка "имя + версия" |
| `connect()` | `Result<Self>` | Подключение к хранилищу |
| `disconnect()` | `Result<()>` | Отключение и очистка |
| `server_time()` | `Result<u32>` | Текущее время сервера (Unix timestamp) |
| `exists()` | `bool` | Проверка существования файла/директории |
| `ls()` | `Result<impl Iterator>` | Перечисление директории |
| `stat()` | `Result<Stat>` | Получение информации о файле/директории |
| `mv()` | `Result<()>` | Перемещение/переименование файла |
| `mkdir()` | `Result<()>` | Создание директории |
| `rm()` | `Result<()>` | Удаление файла/директории |
| `read()` | `Result<Box<dyn io::Read>>` | Чтение файла |
| `write()` | `Result<Box<dyn io::Write>>` | Запись в файл |

### Клиентский слой (`Fs<D>`)

`Fs<D>` — обёртка над драйвером с lock-механизмами, корзиной и UUID. Все операции чтения и записи защищены блокировками.

#### Клиентские методы

| Метод | Возврат | Описание |
|-------|---------|----------|
| `driver_info()` | `String` | Информация о драйвере |
| `time()` | `Result<u32>` | Текущее время сервера |
| `exists()` | `bool` | Проверка существования файла/директории |
| `ls()` | `Result<impl Iterator>` | Перечисление директории |
| `stat()` | `Result<Stat>` | Информация о файле/директории |
| `mv()` | `Result<()>` | Перемещение файла |
| `mkdir()` | `Result<()>` | Создание директории |
| `rm()` | `Result<()>` | Перемещение файла в корзину |
| `read()` | `Result<Box<dyn io::Read>>` | Чтение файла |
| `write()` | `Result<Box<dyn io::Write>>` | Запись в файл |

## 🧩 Типы и структуры

### Stat

Информация о файле или директории:

```rust
pub struct Stat {
    /// Имя файла или директории
    pub name: String,
    /// Путь к файлу/директории
    pub path: PathBuf,
    /// Тип: файл или директория
    pub kind: FileKind,
    /// Размер в байтах
    pub size: u64,
    /// Время последнего изменения
    pub modified: DateTime<Utc>,
    /// Дата создания
    pub created: DateTime<Utc>,
    /// Дата удаления
    pub deleted: Option<DateTime<Utc>>,
}
```

### DriverParams

Параметры подключения к драйверу:

```rust
pub struct DriverParams {
    /// HashMap параметров
    pub params: HashMap<String, String>,
}
```

#### Преобразование DriverParams

```rust
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

### WriteMode

Режимы записи:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteMode {
    /// Записать с перезаписью существующего файла
    Overwrite,
    /// Записать и добавить в конец файла
    Append,
    /// Не создавать файл, если он уже существует
    FailIfExists,
}
```

### LockMode

Режимы блокировок:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockMode {
    /// Блокировка для чтения
    Read,
    /// Блокировка для записи
    Write,
}
```

### FileKind

Типы файлов:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    /// Обычный файл
    File,
    /// Директория
    Dir,
    /// Символическая ссылка
    Link,
}
```

## 🔧 Использование в коде

### Пример 1: Базовая работа с файлами

```rust
use fs4me_local::LocalDriver;
use fs4me_interface::DriverParams;
use fs4me_client::Fs;

fn main() {
    // Подключение к локальной файловой системе
    let driver = LocalDriver::connect(DriverParams::default())
        .unwrap();

    // Создание обёртки с lock-механизмами
    let client = Fs::new(driver);

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

### Пример 2: Работа с ошибками

```rust
use fs4me_local::LocalDriver;
use fs4me_interface::{DriverParams, DriverError};

fn handle_error() {
    match LocalDriver::connect(DriverParams::default()) {
        Ok(driver) => {
            println!("Подключение успешное");
        }
        Err(e) => {
            // Ошибка подключения
            println!("Ошибка: {}", e);

            // Проверяем детали ошибки
            if let DriverError::FopenError { path, reason } = e {
                println!("Путь файла: {:?}", path);
                println!("Причина: {}", reason);
            }
        }
    }
}
```

### Пример 3: Корзина (Trash)

```rust
use fs4me_local::LocalDriver;
use fs4me_interface::DriverParams;
use fs4me_client::{Fs, WriteMode};
use tempfile::tempdir;

fn test_trash() -> anyhow::Result<()> {
    let tmp_dir = tempdir()?;
    let driver = LocalDriver::connect(DriverParams::from([
        ("PATH".to_string(), tmp_dir.path().to_string_lossy().to_string()),
    ]))?;
    let client = Fs::new(driver);

    // Создание файла
    let writer = client.write(&format!("/test.txt", tmp_dir.path()), WriteMode::Overwrite)?;
    writer.write_all(b"Original content")?;

    // Перемещение в корзину
    client.rm("/test.txt")?;

    // Файл больше не существует в корне
    assert!(!client.exists(&format!("/test.txt", tmp_dir.path())));

    // Восстановление из корзины
    let restored_path = format!("/trash/{}.txt", uuid::Uuid::new_v4());
    client.mv(&format!("/test.txt", tmp_dir.path()), restored_path)?;

    let reader = client.read(&restored_path, 0)?;
    let content = reader.read_to_string()?;
    assert_eq!(content, "Original content");

    Ok(())
}
```

### Пример 4: ФФИ-интеграция

```rust
// Подключение с FFI-параметрами
let params = DriverParams::from([
    ("PATH".to_string(), "/tmp".to_string()),
    ("VERBOSE".to_string(), "true".to_string()),
]);

let driver = LocalDriver::connect(params)?;

// Получение информации о драйвере (можно экспортировать через FFI)
let info = driver.info();
```

## 🧪 Тестирование

### Unit-тесты

```bash
# Запуск всех тестов
cargo test

# Запуск тестов с выводом логов
cargo test -- --nocapture

# Запуск тестов только для локального драйвера
cargo test --package fs4me-local
```

### T-тесты

```bash
# Тестирование с T-фреймворком
cargo test --test t_tests
```

### F-тесты

```bash
# Будущая реализация
cargo test --test f_tests
```

### Проверка FFI

```bash
# Будущая реализация
cargo test --test ffi_tests
```

## 🚀 План развития

1. **Реализация модуля `ffi_tests`** — тестирование FFI-экспорта
2. **Поддержка других файловых систем**: SFTP, FTP, WebDav
3. **Макросы для FFI-экспорта** — автоматическая генерация FFI-кода
4. **Асинхронные операции** — поддержка Tokio/async-std
5. **Потоковая обработка** — параллельный доступ к файлам
6. **Документация для разработчиков** — API-документация, примеры, уроки

## 📚 Ресурсы

### Внешние ссылки

- **GitHub**: [fs4me](https://github.com/vladimirovmm/fs4me)
- **Документация**: [Rust docs](https://docs.rs/fs4me-local)
- **Issue**: Создайте Issue для вопросов и предложений

### Примеры кода

- **Benchmarks**: Скоростные тесты
- **Best practices**: Рекомендуемые практики
- **Error handling**: Обработка ошибок

## 🤝 Вклад в проект

1. Fork репозитория
2. Создайте ветку (`git checkout -b feature/AmazingFeature`)
3. Commit изменения (`git commit -m 'Add some AmazingFeature'`)
4. Push в ветку (`git push feature/AmazingFeature`)
5. Откройте Pull Request

## 📄 Лицензия

Проект распространяется по лицензии MIT.

## 📞 Контакты

**Автор:** VMM <vladimirov.m.m@mail.ru>

**GitHub:** [vladimirovmm/fs4me](https://github.com/vladimirovmm/fs4me)

**Вопросы?** — Создайте Issue или отправьте Pull Request!
