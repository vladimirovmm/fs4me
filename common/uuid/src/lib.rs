use std::{
    fmt::{Debug, Display},
    str::FromStr,
};

use fs4me_interface::DriverError;
use rand::random;

/// Уникальный идентификатор клиента.
/// Необходим для ведения логов и реализации блокировок, чтобы связывать клиента с конкретными действиями.
#[derive(PartialEq, Clone, Copy, Eq, Hash)]
pub struct FsUuid {
    /// Идентификатор подключения клиента.
    pub connection_id: u64,
    /// Номер копии клиента.
    pub copy_id: u32,
}

impl Default for FsUuid {
    fn default() -> Self {
        Self {
            connection_id: random(),
            copy_id: random(),
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
impl FsUuid {
    pub fn new_copy_id(&self) -> Self {
        Self {
            connection_id: self.connection_id, // Идентификатор подключения
            copy_id: random(),                 // Номер копии подключения
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

#[cfg(test)]
mod tests {
    use crate::FsUuid;
    use rand::random;
    use std::str::FromStr;

    #[test]
    fn test_parse_uuid() {
        for _ in 0..10 {
            let uuid = FsUuid {
                connection_id: random::<u64>(),
                copy_id: random::<u32>(),
            };
            let uuid_str = format!("{}_{}", uuid.connection_id, uuid.copy_id);

            let result = FsUuid::from_str(&uuid_str).unwrap();
            assert_eq!(uuid, result);
            assert_eq!(uuid_str, result.to_string())
        }
    }

    #[test]
    fn test_copy_clone() {
        let uuid = FsUuid {
            connection_id: random::<u64>(),
            copy_id: random::<u32>(),
        };
        let uuid_with_new_copy_id = uuid.new_copy_id();
        assert!(uuid != uuid_with_new_copy_id);
        assert_eq!(uuid.connection_id, uuid_with_new_copy_id.connection_id);
        assert!(uuid.copy_id != uuid_with_new_copy_id.copy_id);

        let copy_uuid = uuid;
        assert_eq!(uuid, copy_uuid);
    }

    #[test]
    fn test_min_max() {
        FsUuid::from_str(&format!(
            "{connection_id}_{copy_id}",
            connection_id = u64::MIN,
            copy_id = u32::MIN,
        ))
        .unwrap();
        FsUuid::from_str(&format!(
            "{connection_id}_{copy_id}",
            connection_id = u64::MAX,
            copy_id = u32::MAX,
        ))
        .unwrap();
    }

    #[test]
    fn test_invalid_str() {
        for uuid_str in ["1_", "1", "1.1", "1_1_1", "_1"] {
            assert!(FsUuid::from_str(uuid_str).is_err());
        }
    }
}
