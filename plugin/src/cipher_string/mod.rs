pub(crate) struct CipherString<T> {
    pub(crate) inner: Untyped,
    data: PhantomData<fn() -> T>,
}

impl<T> From<Untyped> for CipherString<T> {
    fn from(inner: Untyped) -> Self {
        Self {
            inner,
            data: PhantomData,
        }
    }
}

impl<T> From<CipherString<T>> for Untyped {
    fn from(cipher_string: CipherString<T>) -> Self {
        cipher_string.inner
    }
}

impl<T> Display for CipherString<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.inner, f)
    }
}

impl<T> FromStr for CipherString<T> {
    type Err = untyped::ParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::from(Untyped::from_str(s)?))
    }
}

impl<'de, T> Deserialize<'de> for CipherString<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self::from(Untyped::deserialize(deserializer)?))
    }
}

impl<T> Debug for CipherString<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Debug::fmt(&self.inner, f)
    }
}

impl<T: Stored> CipherString<T> {
    pub(crate) fn decrypt(&self, key: &SymmetricKey) -> Result<T, DecryptError<T::DecodeError>> {
        let bytes = self.inner.decrypt(key).map_err(DecryptError::Decryption)?;
        let res = T::decode(bytes).map_err(DecryptError::Decoding)?;
        Ok(res)
    }
}

#[derive(Debug)]
pub(crate) enum DecryptError<DecodeError> {
    Decryption(untyped::DecryptError),
    Decoding(DecodeError),
}

impl<DecodeError> Display for DecryptError<DecodeError> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decryption(e) => Display::fmt(e, f),
            Self::Decoding(_) => f.write_str("failed to decode cipher string"),
        }
    }
}

impl<DecodeError: 'static + Error> Error for DecryptError<DecodeError> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Decryption(e) => e.source(),
            Self::Decoding(e) => Some(e),
        }
    }
}

pub(crate) trait Stored: Sized {
    fn encode<O, F: FnOnce(&[u8]) -> O>(&self, f: F) -> O;

    type DecodeError;
    fn decode(bytes: Vec<u8>) -> Result<Self, Self::DecodeError>;
}

impl Stored for Vec<u8> {
    fn encode<O, F: FnOnce(&[u8]) -> O>(&self, f: F) -> O {
        f(self)
    }
    type DecodeError = Infallible;
    fn decode(bytes: Vec<u8>) -> Result<Self, Self::DecodeError> {
        Ok(bytes)
    }
}

impl Stored for String {
    fn encode<O, F: FnOnce(&[u8]) -> O>(&self, f: F) -> O {
        f(self.as_bytes())
    }
    type DecodeError = FromUtf8Error;
    fn decode(bytes: Vec<u8>) -> Result<Self, Self::DecodeError> {
        String::from_utf8(bytes)
    }
}

impl Stored for bool {
    fn encode<O, F: FnOnce(&[u8]) -> O>(&self, f: F) -> O {
        f(match *self {
            true => b"true",
            false => b"false",
        })
    }
    type DecodeError = NotBoolean;
    fn decode(bytes: Vec<u8>) -> Result<Self, Self::DecodeError> {
        Ok(match &*bytes {
            b"true" => true,
            b"false" => false,
            _ => return Err(NotBoolean),
        })
    }
}

#[derive(Debug)]
pub(crate) struct NotBoolean;
impl Display for NotBoolean {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("input was not a boolean")
    }
}
impl Error for NotBoolean {}

pub(crate) use untyped::Untyped;
mod untyped;

impl CipherString<SymmetricKey> {
    pub(crate) fn decrypt(
        &self,
        master_key: &MasterKey,
    ) -> Result<SymmetricKey, DecryptSymmetricKeyError> {
        let stretched_key = SymmetricKey::stretch_master(master_key);

        let key_vec = self.inner.decrypt(&stretched_key).map_err(|e| match e {
            untyped::DecryptError::InvalidMac(_) => {
                DecryptSymmetricKeyError::WrongMasterPassword(WrongMasterPassword)
            }
            untyped::DecryptError::Unpadding(e) => DecryptSymmetricKeyError::Unpadding(e),
        })?;
        let key_vec = Zeroizing::new(key_vec);

        if key_vec.len() != SymmetricKey::LEN {
            return Err(DecryptSymmetricKeyError::WrongSize(WrongSymmetricKeySize));
        }

        let mut key = SymmetricKey::zeroed();
        key.0.copy_from_slice(&key_vec);
        Ok(key)
    }
}

#[derive(Debug)]
pub(crate) enum DecryptSymmetricKeyError {
    WrongMasterPassword(WrongMasterPassword),
    Unpadding(block_padding::UnpadError),
    WrongSize(WrongSymmetricKeySize),
}

impl Display for DecryptSymmetricKeyError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("failed to unlock vault")
    }
}

impl Error for DecryptSymmetricKeyError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::WrongMasterPassword(e) => Some(e),
            Self::Unpadding(e) => Some(e),
            Self::WrongSize(e) => Some(e),
        }
    }
}

#[derive(Debug)]
pub(crate) struct WrongMasterPassword;

impl Display for WrongMasterPassword {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("master password is incorrect")
    }
}

impl Error for WrongMasterPassword {}

#[derive(Debug)]
pub(crate) struct WrongSymmetricKeySize;

impl Display for WrongSymmetricKeySize {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("symmetric key was wrong size")
    }
}

impl Error for WrongSymmetricKeySize {}

use crate::symmetric_key::SymmetricKey;
use rofi_bw_common::MasterKey;
use serde::Deserialize;
use serde::Deserializer;
use std::convert::Infallible;
use std::error::Error;
use std::fmt;
use std::fmt::Debug;
use std::fmt::Display;
use std::fmt::Formatter;
use std::marker::PhantomData;
use std::str::FromStr;
use std::string::FromUtf8Error;
use zeroize::Zeroizing;
