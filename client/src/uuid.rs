use std::{
    fmt::{Debug, Display},
    str::FromStr,
};

use fs4me_interface::DriverError;
use rand::random;

#[derive(PartialEq, Copy, Eq, Hash)]
pub struct FsUuid {
    /// Идентификатор подключения.
    pub connection_id: u64,
    /// Номер копии подключения.
    pub copy_id: u32,
}

impl Default for FsUuid {
    fn default() -> Self {
        Self {
            connection_id: random(),
            copy_id: 1,
        }
    }
}

impl Debug for FsUuid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}_{}", self.connection_id, self.copy_id)
    }
}

impl Display for FsUuid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}_{}", self.connection_id, self.copy_id)
    }
}

/// Нестандартная реализация `Clone` для `FsUuid`.
/// Нужно, чтобы при клонировании клиента Fs менялся идентификатор. Но чтобы было понятно, кто родитель
#[allow(clippy::non_canonical_clone_impl)]
impl Clone for FsUuid {
    fn clone(&self) -> Self {
        Self {
            connection_id: self.connection_id, // Идентификатор подключения
            copy_id: self.copy_id + 1,         // Номер копии подключения
        }
    }
}

impl FromStr for FsUuid {
    type Err = DriverError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (connection_id, copy_id) =
            s.split_once('_')
                .ok_or_else(|| DriverError::ParseUuidError {
                    reason: s.to_string(),
                })?;

        let connection_id = connection_id
            .parse()
            .map_err(|e| DriverError::ParseUuidError {
                reason: format!(
                    "Недопустимый формат connection_id: {e:?}. source: {connection_id:?}"
                ),
            })?;
        let copy_id = copy_id.parse().map_err(|e| DriverError::ParseUuidError {
            reason: format!("Недопустимый формат copy_id: {e:?}. source: {copy_id:?}"),
        })?;

        Ok(Self {
            connection_id,
            copy_id,
        })
    }
}

impl AsRef<FsUuid> for FsUuid {
    fn as_ref(&self) -> &FsUuid {
        self
    }
}
