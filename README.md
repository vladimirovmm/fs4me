# fs4me — клиент для работы с файловыми системами.

---

## Обзор

fs4me — это библиотека Rust, предоставляющая унифицированный интерфейс для работы с файлами и директориями через драйверы. В основе лежит паттерн драйвера, позволяющий легко добавлять поддержку различных файловых систем.

**Что реализовано:**
- Локальный драйвер (доступ к файловой системе хоста)
- Fs клиент — обёртка с lock-механизмами и безопасными операциями
- Интерфейсный trait для создания собственных драйверов

---

## Структура проекта

```
fs4me/
├── Cargo.toml
├── Cargo.lock
├── driver/          # Драйверы (локальный, фатальный и т.д.)
│   └── local/       # Реализация локального драйвера
│       └── src/lib.rs
├── interface/       # Базовый интерфейс
│   └── src/lib.rs
├── client/          # Fs клиент с lock-управлением и удалением в корзину
    │   ├── src/
    │   │   ├── lib.rs      # Основная реализация клиента Fs
    │   │   ├── lock.rs     # Механизм lock-файлов (чтение/запись/очередь)
    │   │   ├── trash.rs    # Удаление в корзину (mv в корзину)
    │   │   └── uuid.rs     # UUID для идентификации клиентов
    │   ├── tests/
    │   └── Cargo.toml
└── target/          # Сборка (не входит в исходный код)
```

---

## Fs клиент

Fs клиент представляет собой обёртку над драйвером с автоматическим управлением lock-файлами. Он обеспечивает безопасную работу с файлами, директориями и поддерживает параллельные операции.

### Основные методы

#### Работа с путями

- `exists(path: &Path)` — проверка существования файла или директории
- `stat(path: &Path)` — получение информации о файле (размер, права, дата изменения)
- `read(path: &Path, position: u64 = 0)` — чтение файла с указанной позиции
- `write(path: &Path, data: &[u8], mode: WriteMode)` — запись данных в файл
- `mv(from: &Path, to: &Path)` — перемещение файла или переименование
- `ls(path: &Path)` — перечисление содержимого директории
- `mkdir(path: &Path, recursive: bool)` — создание директории
- `rm(path: &Path)` — удаление файла или непустой директории


---

## Примеры работы с клиентом

### Базовый пример: чтение и запись

```rust
use fs4me_client::Fs;
use fs4me_local::LocalDriver;
use fs4me_interface::WriteMode;

fn main() -> anyhow::Result<()> {
    // Подключаем локальный драйвер
    let driver = LocalDriver::connect(Default::default())?;
    let mut client = Fs<LocalDriver>::from(driver);

    // Проверяем существование файла
    if client.exists("/tmp/test.txt") {
        println!("Файл существует");
    }

    // Получаем информацию о файле
    if let Some(stat) = client.stat("/tmp/test.txt") {
        println!("Размер файла: {} байт", stat.size);
    }

    // Записываем данные в файл
    let data = b"Hello from fs4me client!";
    client.write("/tmp/test.txt", data, WriteMode::Overwrite)?;

    // Читаем файл с позиции 0
    let content = client.read("/tmp/test.txt", 0)?;
    println!("Содержимое: {}", String::from_utf8_lossy(&content));

    // Перечисляем содержимое директории
    let contents = client.ls("/tmp")?;
    for entry in contents {
        println!("{} (тип: {:?})", entry.name, entry.kind);
    }

    Ok(())
}
```

### Создание директории и удаление

```rust
use fs4me_client::Fs;
use fs4me_local::LocalDriver;

fn main() -> anyhow::Result<()> {
    // Подключаем локальный драйвер
    let driver = LocalDriver::connect(Default::default())?;
    let mut client = Fs<LocalDriver>::from(driver);

    // Создаём директории (рекурсивно)
    client.mkdir("/tmp/new_dir", true)?;
    println!("Создан: /tmp/new_dir");

    // Создаём поддиректорию без рекурсии
    client.mkdir("/tmp/new_dir/subdir", false)?;
    println!("Создан: /tmp/new_dir/subdir");

    // Удаляем непустую директорию с содержимым
    client.rm("/tmp/new_dir")?;
    println!("Удалён: /tmp/new_dir с содержимым");

    Ok(())
}
```

### Перемещение и переименование

```rust
use fs4me_client::Fs;
use fs4me_local::LocalDriver;
use fs4me_interface::WriteMode;

fn main() -> anyhow::Result<()> {
    // Подключаем локальный драйвер
    let driver = LocalDriver::connect(Default::default())?;
    let mut client = Fs<LocalDriver>::from(driver);

    // Создаём файл для перемещения
    client.write("/tmp/original.txt", b"test", WriteMode::Overwrite)?;
    println!("Создан: /tmp/original.txt");

    // Перемещаем файл
    client.mv("/tmp/original.txt", "/tmp/moved.txt")?;
    println!("Перемещён: /tmp/original.txt → /tmp/moved.txt");

    // Можно переименовывать и директории
    client.mv("/tmp/new_dir", "/tmp/renamed_dir")?;
    println!("Переименован: /tmp/new_dir → /tmp/renamed_dir");

    Ok(())
}
```

---

## Безопасность

- **Lock-файлы**: поддержка параллельного чтения (`Read`) и эксклюзивной блокировки при записи (`Write`). При записи используется очередь (`WriteQueue`) для ожидания освобождения lock.
- **Удаление в корзину**: `rm` перемещает файл/директорию в корзину, проверяя блокировки только удаляемого пути.
- **Автоматическое освобождение lock**: lock-файлы освобождаются автоматически при выходе из области видимости (`Drop` trait).

---

## Разработчик

**VMM <vladimirov.m.m@mail.ru>**
Лицензия: MIT
Создан: 2024
