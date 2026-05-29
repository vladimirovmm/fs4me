// use eyre::{Context, Result};
// use std::{
//     fs::OpenOptions,
//     io::{self, BufWriter, Seek},
//     path::Path,
// };

// use crate::{
//     LocalDriver,
//     interface::{DriverUnsafeRW, WriteMode},
// };

// impl DriverUnsafeRW for LocalDriver {}
// #[cfg(test)]
// mod tests {

//     use tempfile::tempdir;
//     use tracing::info;
//     use tracing_test::traced_test;

//     use crate::{
//         LocalDriver,
//         interface::{Driver, DriverParams, DriverPathResolver, DriverUnsafeRW, WriteMode},
//     };

//     /// Тестирование работы LocalDriver с файлами.
//     /// Проверяет создание, чтение, запись и перезапись файлов.
//     #[test]
//     #[traced_test]
//     fn test_unsafe_rw() {
//         let tmp_dir = tempdir().unwrap();
//         let root_path = tmp_dir.path();
//         info!("Временная директория: {root_path:?}");
//         let driver = LocalDriver::connect(DriverParams::default()).unwrap();

//         let test_file = root_path.join("tmp.txt");
//         info!("Файл для тестирования: {test_file:?}");

//         assert!(
//             !driver.exists(&test_file),
//             "Файла не должно быть, т.к. директория новая и пустая"
//         );

//         info!("Проверяем, что файл не существует");
//         assert!(!driver.exists(&test_file), "Файл не должен существовать");

//         info!("Создаём файл.");
//         {
//             let mut file = driver
//                 .unsafe_write(&test_file, WriteMode::FailIfExists)
//                 .unwrap();
//             writeln!(&mut file, "a").unwrap();
//         }

//         info!("Проверяем, что файл существует");
//         assert!(
//             driver.exists(&test_file),
//             "Файл должен существовать после записи"
//         );

//         info!("Проверяем, что write с FailIfExists выбрасывает ошибку при существующем файле");
//         assert!(
//             driver
//                 .unsafe_write(&test_file, WriteMode::FailIfExists)
//                 .is_err(),
//             "Должно выбросить ошибку, т.к. файл уже существует"
//         );

//         info!("Проверяем дозапись в конец файла");
//         {
//             let mut file = driver.unsafe_write(&test_file, WriteMode::Append).unwrap();
//             writeln!(&mut file, "b").unwrap();
//         }

//         info!("Проверяем перезапись файла");
//         {
//             let mut file = driver
//                 .unsafe_write(&test_file, WriteMode::Overwrite)
//                 .unwrap();
//             writeln!(&mut file, "c").unwrap();
//         }
//     }
// }
