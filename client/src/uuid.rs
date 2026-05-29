use std::fmt::{Debug, Display};

use rand::random;

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

impl Clone for FsUuid {
    fn clone(&self) -> Self {
        Self {
            connection_id: self.connection_id,
            copy_id: self.copy_id + 1,
        }
    }
}
