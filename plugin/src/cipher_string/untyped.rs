#[derive(Clone)]
pub(crate) struct Untyped {
    iv: [u8; 16],
    ciphertext: Vec<u8>,
    mac: [u8; 32],
}

impl Display for Untyped {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Display::fmt(&2, f)?;
        f.write_str(".")?;
        Base64Display::with_config(&self.iv, base64::STANDARD).fmt(f)?;
        f.write_str("|")?;
        Base64Display::with_config(&self.ciphertext, base64::STANDARD).fmt(f)?;
        f.write_str("|")?;
        Base64Display::with_config(&self.mac, base64::STANDARD).fmt(f)?;
        Ok(())
    }
}

impl FromStr for Untyped {
    type Err = ParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (r#type, rest) = s.split_once(".").ok_or(ParseErrorInner::NoDot)?;
        if r#type != "2" {
            let type_num = r#type.parse::<u32>().ok();
            return Err(ParseErrorInner::UnsupportedEncryptionType(type_num).into());
        }

        let mut parts = rest.splitn(3, "|");
        let (iv, ciphertext, mac) = (|| Some((parts.next()?, parts.next()?, parts.next()?)))()
            .ok_or(ParseErrorInner::NotEnoughSegments)?;
        if parts.next().is_some() {
            return Err(ParseErrorInner::UnexpectedSegment.into());
        }

        let iv = base64_decode_array(iv).map_err(ParseErrorInner::InvalidIv)?;
        let ciphertext = base64::decode(ciphertext).map_err(ParseErrorInner::InvalidCiphertext)?;
        let mac = base64_decode_array(mac).map_err(ParseErrorInner::InvalidMac)?;

        Ok(Self {
            iv,
            ciphertext,
            mac,
        })
    }
}

impl<'de> Deserialize<'de> for Untyped {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> de::Visitor<'de> for Visitor {
            type Value = Untyped;
            fn expecting(&self, f: &mut Formatter<'_>) -> fmt::Result {
                f.write_str("a cipher string")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                v.parse::<Untyped>()
                    .map_err(|e| de::Error::custom(format_args!("{e}: {}", e.0)))
            }
        }
        deserializer.deserialize_str(Visitor)
    }
}

#[derive(Debug)]
pub(crate) struct ParseError(ParseErrorInner);

impl Display for ParseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("failed to parse cipher string")
    }
}

impl Error for ParseError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.0)
    }
}

#[derive(Debug)]
enum ParseErrorInner {
    NoDot,
    UnsupportedEncryptionType(Option<u32>),
    NotEnoughSegments,
    UnexpectedSegment,
    InvalidIv(base64_decode_array::Error),
    InvalidCiphertext(base64::DecodeError),
    InvalidMac(base64_decode_array::Error),
}

impl Display for ParseErrorInner {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoDot => f.write_str("no dot"),
            Self::UnsupportedEncryptionType(Some(v)) => match v {
                0 => f.write_str("unsupported encryption type AES-CBC-256"),
                1 => f.write_str("unsupported encryption type AES-CBC-128-HMAC-SHA256"),
                2 => unreachable!(),
                3 => f.write_str("unsupported encryption type RSA-2048-OAEP-SHA256"),
                4 => f.write_str("unsupported encryption type RSA-2048-OAEP-SHA1"),
                5 => f.write_str("unsupported encryption type RSA-2048-OAEP-SHA256-HMAC-SHA256"),
                6 => f.write_str("unsupported encryption type RSA-2048-OAEP-SHA1-HMAC-SHA256"),
                _ => write!(f, "unsupported encryption type {v}"),
            },
            Self::UnsupportedEncryptionType(None) => f.write_str("unsupported encryption type"),
            Self::NotEnoughSegments => f.write_str("not enough pipe-separated segments"),
            Self::UnexpectedSegment => f.write_str("unexpected pipe-separated segment at end"),
            Self::InvalidIv(_) => f.write_str("IV is invalid"),
            Self::InvalidCiphertext(_) => f.write_str("ciphertext is invalid"),
            Self::InvalidMac(_) => f.write_str("MAC is invalid"),
        }
    }
}

impl Error for ParseErrorInner {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidIv(e) | Self::InvalidMac(e) => Some(e),
            Self::InvalidCiphertext(e) => Some(e),
            _ => None,
        }
    }
}

impl From<ParseErrorInner> for ParseError {
    fn from(inner: ParseErrorInner) -> Self {
        Self(inner)
    }
}

impl Untyped {
    pub(crate) fn encrypt<R: ?Sized + Rng + CryptoRng>(
        key: &SymmetricKey,
        rng: &mut R,
        plaintext: &[u8],
    ) -> Self {
        let iv = rng.gen::<[u8; 16]>();

        let ciphertext =
            <cbc::Encryptor<aes::Aes256>>::new(key.encryption_key().into(), &iv.into())
                .encrypt_padded_vec_mut::<block_padding::Pkcs7>(plaintext);

        let mac = <Hmac<Sha256>>::new_from_slice(key.mac_key())
            .expect("hmac supports any size of key")
            .chain_update(iv)
            .chain_update(&*ciphertext)
            .finalize()
            .into_bytes()
            .into();

        Self {
            iv,
            ciphertext,
            mac,
        }
    }

    fn verify(&self, key: &SymmetricKey) -> Result<(), DecryptError> {
        <Hmac<Sha256>>::new_from_slice(key.mac_key())
            .expect("hmac supports any kind of key")
            .chain_update(self.iv)
            .chain_update(&*self.ciphertext)
            .verify(&self.mac.into())
            .map_err(DecryptError::InvalidMac)?;
        Ok(())
    }

    pub(crate) fn decrypt(&self, key: &SymmetricKey) -> Result<Vec<u8>, DecryptError> {
        self.verify(key)?;

        let mut buf = vec![0; self.ciphertext.len()];
        let len = <cbc::Decryptor<aes::Aes256>>::new(key.encryption_key().into(), &self.iv.into())
            .decrypt_padded_b2b_mut::<block_padding::Pkcs7>(&self.ciphertext, &mut buf)
            .map_err(DecryptError::Unpadding)?
            .len();
        buf.truncate(len);
        Ok(buf)
    }
}

#[derive(Debug)]
pub(crate) enum DecryptError {
    InvalidMac(MacError),
    Unpadding(block_padding::UnpadError),
}

impl Display for DecryptError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("failed to decrypt cipher string")
    }
}

impl Error for DecryptError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidMac(e) => Some(e),
            Self::Unpadding(e) => Some(e),
        }
    }
}

impl Debug for Untyped {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        struct Hex<'bytes>(&'bytes [u8]);
        impl Debug for Hex<'_> {
            fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
                f.write_str("0x")?;
                for byte in self.0 {
                    write!(f, "{byte:02X}")?;
                }
                Ok(())
            }
        }

        f.debug_struct("Untyped")
            .field("iv", &Hex(&self.iv))
            .field("ciphertext", &Hex(&self.ciphertext))
            .field("mac", &Hex(&self.mac))
            .finish()
    }
}

use crate::base64_decode_array;
use crate::SymmetricKey;
use base64::display::Base64Display;
use cipher::BlockDecryptMut;
use cipher::BlockEncryptMut;
use crypto_common::KeyIvInit;
use digest::Mac;
use digest::MacError;
use hmac::Hmac;
use rand::CryptoRng;
use rand::Rng;
use serde::de;
use serde::Deserialize;
use serde::Deserializer;
use sha2::Sha256;
use std::error::Error;
use std::fmt;
use std::fmt::Debug;
use std::fmt::Display;
use std::fmt::Formatter;
use std::str;
use std::str::FromStr;
