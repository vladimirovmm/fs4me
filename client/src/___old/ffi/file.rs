// use eyre::{Context, Result, ensure};
// use rand::random;
// use std::{
//     ffi::c_void,
//     fs::{self, OpenOptions},
//     io::{self, BufWriter, Seek},
//     path::{Path, PathBuf},
//     time::{SystemTime, UNIX_EPOCH},
// };

// use crate::{
//     LocalDriver,
//     interface::{Driver, WriteMode},
// };

// impl Driver for LocalDriver {
//     /// Записывает данные в файл.
//     ///
//     /// @param path - Путь к файлу.
//     /// @param mode - Режим записи.
//     /// @return Result<Box<dyn io::Write>> - Результат: успешная запись или ошибка.
//     fn write_file<P: AsRef<Path>>(&self, path: &P, mode: WriteMode) -> Result<Box<dyn io::Write>> {
//         let mut options = OpenOptions::new();
//         options.write(true); // Создаём, если не существует

//         match mode {
//             WriteMode::FailIfExists => options.create_new(true),
//             WriteMode::Overwrite => options.create(true).truncate(true),
//             WriteMode::Append => options.create(true).append(true),
//         };

//         let file = options.open(path).context("Не удалось открыть файл")?;

//         Ok(Box::new(BufWriter::new(file)))
//     }

//     /// Читает данные из файла.
//     ///
//     /// @param path - Путь к файлу.
//     /// @param position - Позиция в файле, с которой начать чтение.
//     /// @return Result<Box<dyn io::Read>> - Результат: успешное чтение или ошибка.
//     fn read_file<P: AsRef<Path>>(&self, path: &P, position: u64) -> Result<Box<dyn io::Read>> {
//         let mut file = OpenOptions::new()
//             .read(true)
//             .open(path)
//             .context("Не удалось открыть файл")?;
//         file.seek(std::io::SeekFrom::Start(position))
//             .context("Не удалось перейти к позиции")?;
//         // Создаём буферизированный читатель — оптимизирует последующие операции чтения
//         let buf_reader = io::BufReader::new(file);
//         Ok(Box::new(buf_reader))
//     }
// }

// #[cfg(test)]
// mod tests {}
