# fs4me — клиент для работы с файловыми системами.

---

## Обзор

fs4me — это библиотека, предоставляющая унифицированный интерфейс для работы с файлами и директориями через драйвера.

---

## 🚧 Дорожная карта

- [x] Локальный драйвер (доступ к файловой системе хоста)
- [x] Fs клиент — обёртка с lock-механизмами и безопасными операциями
- [x] Интерфейсный trait для создания собственных драйверов
- [x] Генерация FFI API для работы с клиентом для всех реализующих трейт Driver
- [ ] Дополнительные драйверы: SFTP, FTP, WebDAV
- [ ] Поддержка ссылок
- [ ] Поддержка WebAssembly
--

## Fs клиент

Fs клиент представляет собой обёртку над драйвером с автоматическим управлением lock-файлами. Он обеспечивает безопасную работу с файлами, директориями и поддерживает параллельные операции.

### Методы

- `driver_info()` — получение информации о драйвере (имя, версия)
- `time()` — получение текущей серверной даты и времени в Unix timestamp
- `exists(path: &Path)` — проверка существования файла или директории
- `stat(path: &Path)` — получение информации о файле (размер, дата изменения) или директории (дата изменения)
- `read(path: &Path, position: u64)` — возвращает буферизированный читатель для чтения файла с указанной позиции
- `write(path: &Path, mode: WriteMode)` — возвращает буферизированный писатель для записи данных в файл с указанным режимом
- `rename(from: &Path, to: &Path)` — перемещение файла или переименование
- `ls(path: &Path)` — перечисление содержимого директории (возвращает итератор путей файлов и директорий)
- `mkdir(path: &Path, recursive: bool)` — создание директории (если `recursive: true`, создаются все промежуточные директории)
- `rm(path: &Path)` — перемещение файла или директории в корзину
- `copy_file(from: &Path, to: &Path)` - Скопировать файл
- `copy(from:&Path,to:&Path)` - Рекурсивно копирует файл/директорию.

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
    client.rename("/tmp/original.txt", "/tmp/moved.txt")?;
    println!("Перемещён: /tmp/original.txt → /tmp/moved.txt");

    // Можно переименовывать и директории
    client.rename("/tmp/new_dir", "/tmp/renamed_dir")?;
    println!("Переименован: /tmp/new_dir → /tmp/renamed_dir");

    Ok(())
}
```

---

## Безопасность

- **Lock-файлы**: поддержка параллельного чтения (`Read`) и эксклюзивной блокировки при записи (`Write`). При записи используется очередь (`WriteQueue`) для ожидания освобождения lock.
- **Удаление в корзину**: `rm` перемещает файл/директорию в корзину, проверяя блокировки только удаляемого пути.
- **Автоматическое освобождение lock**: lock-файлы освобождаются автоматически при выходе из области видимости (`Drop` trait).
