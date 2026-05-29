use eyre::{Context, ContextCompat, Result, ensure};
use std::{
    fmt::Debug,
    io,
    path::{Path, PathBuf},
};
use tracing::error;

mod writer;
pub use crate::interface::{writer::DriverWriter, writer::lock_file_path};

/// Получить путь к корзине для указанного пути.
/// Корзина находится в родительской директории указанного пути.
/// Имя корзины - ".trash".
/// Если корзина не существует, она будет создана.
///
/// @param path - Путь к директории.
/// @return Result<PathBuf> - Результат: путь к корзине или ошибка.
pub fn trash_dir<P: AsRef<Path>, D: OldDriver>(driver: &D, path: P) -> Result<PathBuf> {
    let path = path.as_ref();

    let parent_dir = path
        .parent()
        .context("Нельзя удалить корневую директорию")?;
    let trash_dir = parent_dir.join(".trash");
    if !driver.exists(&trash_dir) {
        driver.mkdir(&trash_dir, false)?;
    }

    Ok(trash_dir)
}

/// Получить путь для удаления файла/директории.
///
/// @param path - Путь к файлу или директории.
/// @return Result<PathBuf> - Результат: путь для удаления или ошибка.
///
/// Пример:
/// ./a/b/c -> ./a/b/.trash/c
///
/// Если с уже существует, добавляется суффикс `_<index>`.
/// ./a/b/c -> ./a/b/.trash/c_1
///
pub fn trash_path<P: AsRef<Path>, D: OldDriver>(driver: &D, path: P) -> Result<PathBuf> {
    let path = path.as_ref();
    let trash_dir = trash_dir(driver, path).context("Не удалось получить путь до корзины")?;

    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .context("Не удалось получить имя директории")?;

    // Новый путь
    let mut new_path = trash_dir.join(file_name);
    let mut index = 0;
    while driver.exists(&new_path) {
        index += 1;
        new_path = trash_dir.join(format!("{}_{}", file_name, index));
    }

    Ok(new_path)
}

// pub trait OldDriver: Sized + Clone {
//     /// Возвращает название драйвера.
//     fn name(&self) -> &str;

//     /// Возвращает версию драйвера.
//     fn version(&self) -> &str;

//     /// Возвращает отладочную информацию о драйвере.
//     ///  - название
//     ///  - версия
//     ///
//     /// Формат:
//     ///   name1=value1
//     ///   name2=value2
//     ///
//     /// Пример:
//     ///   name=LocalDriver
//     ///   version=1.0.0
//     fn info(&self) -> String {
//         format!("name={}\nversion={}", self.name(), self.version())
//     }

//     /// Подключение к файловому хранилищу с указанными параметрами.
//     fn connect<P: Into<DriverParams>>(params: P) -> Result<Self>;

//     /// Отключение от файлового хранилища.
//     ///
//     /// При реализации обязательно вызывайте его для drop
//     fn disconnect(&self) -> Result<()>;

//     /// Возвращает время сервера в формате Unix timestamp.
//     ///
//     /// @return Result<u32> - Результат: Unix timestamp в секундах
//     fn server_time(&self) -> Result<u32>;

//     /// Возвращает интератор путей файлов и директорий в указанной директории.
//     ///
//     /// @param path - Путь к директории.
//     /// @return Result<impl Iterator<Item = PathBuf>> - Результат: интератор путей
//     ///
//     /// Пример использования:
//     /// ```ignore
//     /// for entry in fs.ls("/some/path")? {
//     ///     println!("{:?}", entry);
//     /// }
//     /// ```
//     fn ls<P: AsRef<Path>>(&self, path: P) -> Result<impl Iterator<Item = PathBuf>>;

//     /// Проверяет существование файла или директории.
//     ///
//     /// @param path - Путь к файлу или директории.
//     /// @return bool - Результат: true, если файл или директория существует, false - если нет.
//     fn exists<P: AsRef<Path>>(&self, path: P) -> bool;

//     /// Перемещает файл или директорию из одного пути в другой.
//     ///
//     /// @param from - Исходный путь.
//     /// @param to - Целевой путь.
//     /// @return Result<()> - Результат: успех или ошибка
//     fn rename<P: AsRef<Path>>(&self, from: P, to: P) -> Result<()>;

//     /// Создает директорию.
//     ///
//     /// @param path - Путь к директории.
//     /// @param recursive - Рекурсивное создание. Создает все промежуточные директории.
//     /// @return Result<()> - Результат: успешное создание или ошибка.
//     fn mkdir<P: AsRef<Path>>(&self, path: P, recursive: bool) -> Result<()>;

//     /// Перемещает директорию/файл в корзину.
//     /// Корзина необходима для быстрого удаления директории, чтобы минимизировать возможность обращения к содержимому в процессе удаления.
//     /// Не забудьте реализовать очистку корзины.
//     ///
//     /// @param path - Путь к директории.
//     /// @return Result<()> - Результат: успешное удаление или ошибка.
//     fn rm<P: AsRef<Path>>(&self, path: P) -> Result<()> {
//         let path = path.as_ref();
//         ensure!(self.exists(path), "Директория не существует");

//         let new_path = trash_path(self, path)?;
//         self.rename(path, &new_path)
//             .context("Ошибка при перемещении в корзину")
//     }

//     /// Получить инофрмацию о файле/директории.
//     ///
//     /// @param path - Путь к файлу или директории.
//     /// @return Result<Stat> - Результат: информация о файле/директории или ошибка.
//     fn stat<P: AsRef<Path>>(&self, path: P) -> Result<Stat>;

//     /// Записывает данные в файл.
//     ///
//     /// @param path - Путь к файлу.
//     /// @param mode - Режим записи.
//     /// @return Result<Box<dyn io::Write>> - Результат: успешная запись или ошибка.
//     fn unsafe_write<P: AsRef<Path>>(&self, path: &P, mode: WriteMode)
//     -> Result<Box<dyn io::Write>>;

//     /// Записывает данные в файл. Есть несколько режимов записи.
//     ///
//     /// Принцип защиты от одновременного доступа к файлу осуществляется через lock файла
//     /// ./a/file.txt -> ./a/.file.txt.lock
//     ///
//     /// @param path - Путь к файлу.
//     /// @param mode - Режим записи.
//     /// @return Result<Box<dyn io::Write>> - Результат: успешная запись или ошибка.
//     fn write<P: AsRef<Path>>(&self, path: &P, mode: WriteMode) -> Result<DriverWriter<Self>> {
//         DriverWriter::open(Box::new(self.clone()), path.as_ref(), mode)
//     }

//     /// Читает данные из файла.
//     ///
//     /// @param path - Путь к файлу.
//     /// @param position - Позиция в файле, с которой начать чтение.
//     /// @return Result<Box<dyn io::Read>> - Результат: успешное чтение или ошибка.
//     fn read<P: AsRef<Path>>(&self, path: &P, position: u64) -> Result<Box<dyn io::Read>>;
// }
