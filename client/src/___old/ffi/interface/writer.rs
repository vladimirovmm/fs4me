// use std::{
//     io,
//     path::{Path, PathBuf},
//     time::{SystemTime, UNIX_EPOCH},
// };

// use eyre::{Context, ContextCompat, Result, bail, ensure};
// use tracing::error;

// use crate::interface::{OldDriver, Stat, WriteMode};

// fn now() -> u32 {
//     SystemTime::now()
//         .duration_since(UNIX_EPOCH)
//         .expect("Не удалось получить Unix TimeStamp")
//         .as_secs() as u32
// }

// /// Путь до файла блокировки
// ///
// /// @param path Путь к файлу, который нужно заблокировать.
// /// @return `Ok(())` если блокировка успешно установлена.
// pub fn lock_file_path<P: AsRef<Path>>(path: P) -> Result<PathBuf> {
//     let path = path.as_ref();

//     let file_name = path
//         .file_name()
//         .and_then(|n| n.to_str())
//         .map(|name| format!(".{name}.lock"))
//         .context("Не удалось получить имя файла")?;
//     path.parent()
//         .map(|parent| parent.join(file_name))
//         .context("Не удалось получить родительскую директорию")
// }

// pub struct DriverWriter<D: OldDriver> {
//     /// Драйвер, который используется для записи.
//     driver: Box<D>,
//     /// Путь к файлу, который открыт для записи.
//     path: PathBuf,
//     /// Поток для записи данных в файл.
//     writer: Box<dyn io::Write>,
// }

// impl<D: OldDriver> io::Write for DriverWriter<D> {
//     fn flush(&mut self) -> io::Result<()> {
//         self.writer.flush()
//     }

//     fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
//         self.writer.write(buf)
//     }
// }

// /// Структура `Writer` представляет собой инструмент для записи данных в файл.
// ///
// /// @todo - обновлять LockFile, если запись длится более 5 мин
// impl<D: OldDriver> DriverWriter<D> {
//     /// Создает новый `Writer` для записи данных в файл.
//     ///
//     /// @param driver Драйвер, который используется для записи.
//     /// @param path Путь к файлу, который открыт для записи.
//     /// @param mode Режим записи.
//     ///
//     /// @return Новый `Writer` для записи данных в файл.
//     pub fn open<P: Into<PathBuf>>(driver: Box<D>, path: P, mode: WriteMode) -> Result<Self> {
//         let path = path.into();

//         ensure!(
//             !path.exists() || mode != WriteMode::FailIfExists,
//             "Файл уже существует {path:?}"
//         );

//         let lock_path = lock_file_path(&path)?;
//         if driver.exists(&lock_path) {
//             let Stat::File { modified, .. } = driver.stat(&lock_path)? else {
//                 bail!("LockFile не может быть директорией");
//             };
//             let now = driver.server_time()?;
//             // Если более 5 минут, удалить LockFile и продолжить
//             ensure!(now - modified > 5 * 60, "Файл {path:?} заблокирован");

//             driver
//                 .rm(&lock_path)
//                 .with_context(|| format!("Снятие блокировки с файла {path:?}"))?;
//         }
//         // Установить блокировку на файл
//         {
//             let mut writer = driver
//                 .unsafe_write(&lock_path, WriteMode::FailIfExists)
//                 .with_context(|| format!("Установка блокировки на файл {path:?}"))?;
//             writeln!(writer, "{}", now())
//                 .with_context(|| format!("Ошибка записи в LockFile {lock_path:?}"))?;
//         }

//         let writer = driver
//             .unsafe_write(&path, mode)
//             .with_context(|| format!("Открытие файла для записи {path:?}"))?;

//         Ok(Self {
//             driver,
//             path,
//             writer,
//         })
//     }

//     /// Закрывает файл и освобождает ресурсы.
//     pub fn close(mut self) -> Result<()> {
//         self.writer.flush().with_context(|| {
//             format!(
//                 "Ошибка сброса буфера записи для файла {path:?}",
//                 path = self.path
//             )
//         })
//     }
// }

// impl<D: OldDriver> Drop for DriverWriter<D> {
//     fn drop(&mut self) {
//         if let Err(err) = lock_file_path(&self.path).and_then(|lock_path| {
//             self.driver
//                 .rm(&lock_path)
//                 .with_context(|| format!("Удаление LockFile {lock_path:?}"))
//         }) {
//             error!(
//                 "Ошибка при снятии блокировки с файла: {err}\n\
//                 path={path:?}",
//                 path = &self.path
//             );
//         }
//     }
// }

// #[cfg(test)]
// mod tests {
//     use std::io::Write;
//     use tempfile::tempdir;
//     use tracing::info;
//     use tracing_test::traced_test;

//     use crate::{
//         LocalDriver,
//         interface::{DriverParams, OldDriver, lock_file_path},
//     };

//     #[test]
//     #[traced_test]
//     fn test_lock() {
//         let tmp_dir = tempdir().unwrap();
//         info!("Временная директория: {:?}", tmp_dir.path());

//         let driver = LocalDriver::connect(DriverParams::default()).unwrap();

//         let root = tmp_dir.path();
//         let file_path = root.join("demo.txt");
//         info!("Создание файла {file_path:?}. Только если его нет");

//         let mut write_buf = driver
//             .write(&file_path, crate::interface::WriteMode::FailIfExists)
//             .unwrap();
//         assert!(
//             driver.exists(&file_path),
//             "Файл {file_path:?} должен существовать после его открытия"
//         );
//         let lock_file = lock_file_path(&file_path).unwrap();
//         info!(
//             "Файл создан и должен быть заблокирован только для записи. Файл блокировки {lock_file:?}"
//         );
//         assert!(
//             driver.exists(&lock_file),
//             "Файл {lock_file:?} должен существовать после открытия файла"
//         );

//         info!("Попытка открыть параллельную запись");
//         assert!(
//             driver
//                 .write(&file_path, crate::interface::WriteMode::Overwrite)
//                 .is_err(),
//             "Нельзя открыть файл пока он открыт для записи"
//         );

//         info!("Запись в файл");
//         writeln!(&mut write_buf, "Hello, World!").unwrap();

//         write_buf.close().unwrap();

//         assert!(
//             !driver.exists(&lock_file),
//             "Блокировка должна быть снята после закрытия файла"
//         );

//         info!("Тестирование на ошибку создать если файла не существует");
//         assert!(
//             driver
//                 .write(&file_path, crate::interface::WriteMode::FailIfExists)
//                 .is_err(),
//             "Нельзя создать файл если он уже существует"
//         );

//         info!("Тестирование на перезапись файла");
//         let mut write_buf = driver
//             .write(&file_path, crate::interface::WriteMode::Overwrite)
//             .unwrap();
//         writeln!(&mut write_buf, "a").unwrap();
//         drop(write_buf);

//         info!("Тестирование на дозапись файла");
//         let mut write_buf = driver
//             .write(&file_path, crate::interface::WriteMode::Append)
//             .unwrap();
//         writeln!(&mut write_buf, "b").unwrap();
//         drop(write_buf);

//         // TODO добавить чтение содержимого файла
//     }
// }
