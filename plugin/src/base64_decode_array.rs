pub(crate) fn base64_decode_array<const N: usize>(source: &str) -> Result<[u8; N], Error> {
    assert_ne!(N, 0);
    if source.len() != (N + 2) / 3 * 4 {
        return Err(Error::WrongSize(WrongSize));
    }
    match (N % 3, &source.as_bytes()[source.len() - 2..]) {
        (0, &[a, b]) if a != b'=' && b != b'=' => {}
        (1, &[b'=', b'=']) => {}
        (2, &[a, b'=']) if a != b'=' => {}
        _ => return Err(Error::WrongSize(WrongSize)),
    }

    let mut buf = [0; N];
    let bytes = base64::decode_config_slice(source, base64::STANDARD, &mut buf)
        .map_err(Error::InvalidBase64)?;
    assert_eq!(bytes, N);

    Ok(buf)
}

#[derive(Debug)]
pub(crate) enum Error {
    InvalidBase64(base64::DecodeError),
    WrongSize(WrongSize),
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("failed to decode fixed-base base 64")
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidBase64(e) => Some(e),
            Self::WrongSize(e) => Some(e),
        }
    }
}

#[derive(Debug)]
pub(crate) struct WrongSize;

impl Display for WrongSize {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("input data was wrong size")
    }
}

impl std::error::Error for WrongSize {}

use std::fmt;
use std::fmt::Display;
use std::fmt::Formatter;
