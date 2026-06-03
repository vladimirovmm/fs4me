use std::io::{self, Read, Write};

use fs4me_interface::Driver;
use fs4me_lock::MultiLock;

/// Обёртка для буфера записи, гарантирующая автоматическое снятие блокировки файла.
/// При выходе из области видимости (например, при завершении `drop`) блокировка файла будет корректно разблокирована.
/// Это позволяет не беспокоиться о ручном снятии блокировки файлом.
pub struct DriverBufferWrite<D: Driver> {
    pub lock: MultiLock<D>,
    pub write: Box<dyn Write>,
}

impl<D: Driver> Write for DriverBufferWrite<D> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.write.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.write.flush()
    }
}

/// Обёртка для буфера чтения, гарантирующая автоматическое снятие блокировки файла.
/// При выходе из области видимости (например, при завершении `drop`) блокировка файла будет корректно разблокирована.
/// Это позволяет не беспокоиться о ручном снятии блокировки файлом.
pub struct DriverBufferReed<D: Driver> {
    pub lock: MultiLock<D>,
    pub read: Box<dyn Read>,
}

impl<D: Driver> Read for DriverBufferReed<D> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.read.read(buf)
    }
}
