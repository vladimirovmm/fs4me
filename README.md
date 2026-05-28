# fs4me — Rust файловый драйвер

**fs4me** — Rust-библиотека для работы с локальными файловыми системами.
Предоставляет унифицированный интерфейс для операций с файлами и директориями, включая FFI-экспорт для интеграции с C-программами.

[![Edition](https://img.shields.io/badge/rust-2024-blue.svg)](https://doc.rust-lang.org/edition-guide/)
[![API](https://img.shields.io/badge/API-unsafe-yellow.svg)](https://doc.rust-lang.org/reference/items/functions.html)

---

## 🎯 Обзор

fs4me предоставляет:

- **Локальный драйвер** для работы с файловой системой текущего пользователя (fs4me_local). В будущем планируется добавить поддержку других файловых систем.
- **Унифицированный trait `Driver`** для простой разработки дополнительных драйверов
- **FFI-экспорт** с C-подобным API для интеграции с другими языками.

### Пример использования

```rust
use fs4me_local::LocalDriver;

fn main() {
    // Подключение к локальной файловой системе
    let driver = LocalDriver::connect("PATH=/tmp")
        .unwrap();

    // Перечисление директории
    let files: Vec<_> = driver.ls("/tmp")
        .unwrap()
        .filter_map(|p| p.to_str())
        .collect();


    // Запись данных
    driver.write("/tmp/test.txt", "Hello, fs4me!")
        .unwrap();
    // Чтение файла
    let data = driver.read("/tmp/test.txt", 0)
        .unwrap();
}
```

---

## 📦 Установка

```bash
# Клонирование
git clone https://github.com/username/fs4me.git
cd fs4me

# Построение
cargo build --release

# Использование в других проектах
# В Cargo.toml
[dependencies]
fs4me_local = { path = "./driver/local" }
```

---

## 🏗️ Архитектура

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

## 📜 Интерфейс (Trait `Driver`)

Trait `Driver` определяет стандартный набор методов для работы с файлами:

| Метод | Тип | Описание |
|-------|-----|----------|
| `name()` | `&str` | Имя драйвера |
| `version()` | `&str` | Версия драйвера |
| `info()` | `String` | Информация (имя, версия) |
| `connect()` | `Result<Self>` | Подключение к хранилищу |
| `disconnect()` | `Result<()>` | Отключение и очистка |
| `server_time()` | `Result<u64>` | Текущее время сервера (UNIX timestamp) |
| `ls<P>()` | `Result<Iterator>` | Перечисление директории |
| `exists<P>()` | `bool` | Проверка существования файла/директории |
| `mkdir<P>()` | `Result<()>` | Создание директории |
| `rmdir<P>()` | `Result<()>` | Удаление директории |
| `read<P>()` | `Result<Vec<u8>>` | Чтение файла |
| `write<P, D>()` | `Result<()>` | Запись в файл |
| `append<P, D>()` | `Result<()>` | Дописывание в файл |
| `delete<P>()` | `Result<()>` | Удаление файла |
| `rename<P, NP>()` | `Result<()>` | Переименование/перемещение файла |

### DriverParams
Нужен для передачи параметров подключения к драйверу.
Структура параметров подключения поддерживает несколько форматов представления:

```rust
// Из HashMap
let params = HashMap::from([
    ("PATH".to_string(), "/some/path".to_string()),
    ("VERBOSE".to_string(), "true".to_string()),
]);

// Из строки
let params = "PATH=/some/path\nVERBOSE=true\nREAD_ENABLED=true";

// Из C-строки
let params = CString::new("PATH=/some/path").unwrap();
```

---

## 🔌 FFI API (C-подобный интерфейс)

Для интеграции с C-программами предоставлен набор низкоуровневых функций:

| Функция | Тип | Описание |
|---------|-----|----------|
| `client_connect` | `*mut c_void` | Подключение к файловому хранилищу |
| `client_disconnect` | `()` | Отключение и освобождение ресурсов |
| `client_get_info` | `*const c_char` | Получение информации о драйвере |
| `client_read` | `*mut c_char` | Чтение файла (возвращает `malloc`-ированные данные) |
| `client_write` | `()` | Запись данных в файл |
| `free_c_string` | `()` | Освобождение C-строки |

### Пример C-кода

```c
// Подключение
void *client = client_connect("key=value");

// Получение информации
const char *info = client_get_info(client);
free_c_string(info);

// Чтение файла
const char *content = client_read(client, "/tmp/demo.txt");
// ... использовать content ...
free_c_string(content);

// Отключение
client_disconnect(client);
```

---

## 🔒 Безопасность

- ✅ Удаление непустых директорий происходит через переименование (избегание конфликтов)
- ✅ FFI-функции проверяют NULL-указатели
- ✅ Ресурсы автоматически освобождаются в области видимости Rust
- ✅ Ошибки содержат контекст (функция, путь, детали)

---

## 🧪 Тестирование

```bash
# Unit-тесты
cargo test

# FFI-тесты (будущая реализация)
cargo test --package fs4me-ffi-tests
```

Существующие тесты:
- `test_driver_info` — проверка имени и версии
- `test_time` — проверка времени сервера
- `test_work_with_directory` — комплексный тест директорий
- `test_read_write` — тесты операций с файлами
- `test_removal_safety` — проверка безопасного удаления

---

## 📊 Зависимости

### Основные
- `eyre` — обработка ошибок
- `rand` — генерация случайных чисел (для тестов)

### Dev-зависимости
- `tracing` — логирование
- `tracing-subscriber` — JSON-логирование
- `tempfile` — временные директории для тестов

---

## 🚀 План развития

1. Реализовать модуль `ffi_tests`
2. Добавить поддержку `SFTP`, `FTP`,`WebDav` и других файловых систем
3. Реализовать общий макрос для FFI-экспорта
4. Полная документация для разработчиков

---

## 📄 Лицензия

Проект распространяется по лицензии MIT.

---

## 📞 Контакты

**Автор:** VMM <vladimirov.m.m@mail.ru>

**GitHub:** [vladimirovmm/fs4me](https://github.com/vladimirovmm/fs4me)

**Вопросы?** — Создайте Issue или отправьте Pull Request!
