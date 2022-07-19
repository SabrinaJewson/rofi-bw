//! Byte stream traits.

pub trait Writer {
    type Error;
    fn write(&mut self, buf: &[u8]) -> Result<(), Self::Error>;
}

impl<W: ?Sized + io::Write> Writer for W {
    type Error = io::Error;
    fn write(&mut self, buf: &[u8]) -> Result<(), Self::Error> {
        self.write_all(buf)
    }
}

pub trait Reader {
    type Error;
    fn read(&mut self, buf: &mut [u8]) -> Result<(), Self::Error>;
    fn read_array<const N: usize>(&mut self) -> Result<[u8; N], Self::Error> {
        let mut array = [0; N];
        self.read(&mut array)?;
        Ok(array)
    }
    fn read_box(&mut self, n: usize) -> Result<Box<[u8]>, Self::Error> {
        let mut bytes = vec![0; n].into_boxed_slice();
        self.read(&mut bytes)?;
        Ok(bytes)
    }
}

impl<R: ?Sized + io::Read> Reader for R {
    type Error = io::Error;
    fn read(&mut self, buf: &mut [u8]) -> Result<(), Self::Error> {
        self.read_exact(buf)
    }
}

use std::io;
