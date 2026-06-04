use std::path::PathBuf;

use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum DriverError {
    #[error("Ошибка при получения полного пути {path:?}. {reason:?}")]
    PathResolutionError { path: PathBuf, reason: String },

    #[error("Путь {0} существует")]
    PathExistsError(PathBuf),

    #[error("Путь {0} не существует")]
    PathNotExistsError(PathBuf),

    #[error("{0} Не является директорией")]
    NotADirectoryError(PathBuf),

    #[error("Ошибка при чтении директории. Путь:{path:?}. {reason:?}")]
    ReadDirError { path: PathBuf, reason: String },

    #[error("Не удалось получить родительскую директорию для {0:?}.")]
    ParentDirError(PathBuf),

    #[error("Не удалось получить имя файла/директории для {0:?}.")]
    FileNameError(PathBuf),

    #[error("Ошибка при получении времени. {0}")]
    ServerTimeError(String),

    #[error("Ошибка при получении информации о файле/директории. Путь:{path:?}. {reason:?}")]
    StatError { path: PathBuf, reason: String },

    #[error(
        "Ошибка при получении даты последней модификации файла/директории. Путь: {path:?}. {reason:?}"
    )]
    LastModifiedError { path: PathBuf, reason: String },

    #[error(
        "Не удалось переместить/переименовать файл/директорию. Путь:{from:?}->{to:?}. {reason:?}"
    )]
    MvError {
        from: PathBuf,
        to: PathBuf,
        reason: String,
    },

    #[error("Не удаётся создать директорию {path:?}. {reason:?}")]
    MkdirError { path: PathBuf, reason: String },

    #[error("Ошибка при удалении файла/директории. Путь: {path:?}. {reason:?}")]
    RmError { path: PathBuf, reason: String },

    #[error("Ошибка при открытии файла. Путь: {path:?}. {reason:?}")]
    FopenError { path: PathBuf, reason: String },

    #[error("Перемещение курсора во время чтения файла. Путь: {path:?}. {reason:?}")]
    ReadSeekError { path: PathBuf, reason: String },

    #[error("Ошибка при записи в файл. Путь: {path:?}. {reason:?}")]
    WriteError { path: PathBuf, reason: String },

    #[error("Ошибка при блокировке. Путь: {path:?}. Режим: {mode:?}")]
    LockedError { path: PathBuf, mode: String },

    #[error("Lock файл заблокирован для записи. Путь: {0:?}")]
    LockFileBLocked(PathBuf),

    #[error("Ошибка при блокировке. Временная блокировка уже существует. Путь: {0:?}")]
    TempLockExistsError(PathBuf),

    #[error("Ошибка при блокировке. Блокировка была изменена. Путь: {0}.")]
    LockChangedError(PathBuf),

    #[error("Ошибка при разборе файла блокировки. {reason:?}")]
    ParseLockError { reason: String },

    #[error("Ошибка при конвертации строки в UUID. {reason:?}")]
    ParseUuidError { reason: String },

    #[error("Ошибка при копировании. {from:?}->{to:?} {reason:?}")]
    CopyError {
        from: PathBuf,
        to: PathBuf,
        reason: String,
    },

    #[error("Путь должен быть файлом. Путь: {0:?}")]
    PathNotFileError(PathBuf),

    #[error("Ошибка при обновлении времени последнего изменения файла. Путь: {path:?}. {reason:?}")]
    UpdateFileModifiedTimeError { path: PathBuf, reason: String },
}
