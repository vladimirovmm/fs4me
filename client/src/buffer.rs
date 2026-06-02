use std::io::{self, Read, Write};

use fs4me_interface::Driver;

use crate::lock::Lock;

/// Обёртка для буфера записи, гарантирующая автоматическое снятие блокировки файла.
/// При выходе из области видимости (например, при завершении `drop`) блокировка файла будет корректно разблокирована.
/// Это позволяет не беспокоиться о ручном снятии блокировки файлом.
pub struct DriverBufferWrite<'a, D: Driver> {
    pub lock: Lock<'a, D>,
    pub write: Box<dyn Write>,
}

impl<'a, D: Driver> Write for DriverBufferWrite<'a, D> {
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
pub struct DriverBufferReed<'a, D: Driver> {
    pub lock: Lock<'a, D>,
    pub read: Box<dyn Read>,
}

impl<'a, D: Driver> Read for DriverBufferReed<'a, D> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.read.read(buf)
    }
}
