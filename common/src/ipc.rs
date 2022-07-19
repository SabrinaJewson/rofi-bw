/// The name of the environment variable used to pass the FD of the communication pipe
/// to the child process.
pub const PIPE_FD_ENV_VAR: &str = "ROFI_BW_PIPE_FD";

pub use handshake::Handshake;
pub mod handshake {
    pub fn write<W: Writer>(
        writer: &mut W,
        master_key: &MasterKey,
        data: &[u8],
    ) -> Result<(), WriteError<W::Error>> {
        writer.write(&*master_key.0)?;
        writer.write(&u32::try_from(data.len()).unwrap().to_le_bytes())?;
        writer.write(data)?;
        Ok(())
    }

    #[derive(Debug)]
    pub struct WriteError<E>(pub E);

    impl<E> From<E> for WriteError<E> {
        fn from(error: E) -> Self {
            Self(error)
        }
    }

    impl<E> Display for WriteError<E> {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            f.write_str("failed to write handshake")
        }
    }

    impl<E: 'static + Error> Error for WriteError<E> {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            Some(&self.0)
        }
    }

    pub struct Handshake {
        pub master_key: MasterKey,
        pub data: Box<[u8]>,
    }

    pub fn read<R: Reader>(reader: &mut R) -> Result<Handshake, ReadError<R::Error>> {
        let mut master_key_and_data_len = Zeroizing::new([0; MasterKey::LEN + 4]);
        reader.read(&mut *master_key_and_data_len)?;

        let mut master_key = MasterKey::zeroed();
        master_key
            .0
            .copy_from_slice(&master_key_and_data_len[..MasterKey::LEN]);

        let data_len = &master_key_and_data_len[MasterKey::LEN..];
        let data_len = u32::from_le_bytes(data_len.try_into().unwrap());

        let data = reader.read_box(data_len as usize)?;

        Ok(Handshake { master_key, data })
    }

    #[derive(Debug)]
    pub struct ReadError<E>(E);

    impl<E> From<E> for ReadError<E> {
        fn from(error: E) -> Self {
            Self(error)
        }
    }

    impl<E> Display for ReadError<E> {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            f.write_str("failed to read handshake")
        }
    }

    impl<E: 'static + Error> Error for ReadError<E> {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            Some(&self.0)
        }
    }

    use crate::stream::Reader;
    use crate::stream::Writer;
    use crate::MasterKey;
    use std::error::Error;
    use std::fmt;
    use std::fmt::Display;
    use std::fmt::Formatter;
    use zeroize::Zeroizing;
}

pub use menu_request::MenuRequest;
/// A request from the menu to the parent process.
pub mod menu_request {
    #[derive(Debug, Clone, Copy)]
    pub enum MenuRequest<CopyString> {
        Copy(CopyString),
        Sync,
        Lock,
        LogOut,
        Exit,
    }

    impl MenuRequest<&str> {
        pub fn write<W: Writer>(&self, writer: &mut W) -> Result<(), WriteError<W::Error>> {
            match self {
                Self::Copy(data) => {
                    writer.write(&[0])?;
                    writer.write(&u32::try_from(data.len()).unwrap().to_le_bytes())?;
                    writer.write(data.as_bytes())?;
                }
                Self::Sync => writer.write(&[1])?,
                Self::Lock => writer.write(&[2])?,
                Self::LogOut => writer.write(&[3])?,
                Self::Exit => writer.write(&[4])?,
            }
            Ok(())
        }
    }

    impl MenuRequest<Box<str>> {
        pub fn read<R: Reader>(reader: &mut R) -> Result<Self, ReadError<R::Error>> {
            let [request_type] = reader.read_array()?;
            Ok(match request_type {
                0 => {
                    let len = u32::from_le_bytes(reader.read_array()?) as usize;
                    let data = reader.read_box(len)?;
                    let data = String::from_utf8(Vec::from(data))
                        .map_err(|e| ReadError::CopyNonUtf8(CopyNonUtf8(e.utf8_error())))?;
                    Self::Copy(data.into_boxed_str())
                }
                1 => Self::Sync,
                2 => Self::Lock,
                3 => Self::LogOut,
                4 => Self::Exit,
                _ => {
                    return Err(ReadError::UnknownRequestType(UnknownRequestType(
                        request_type,
                    )))
                }
            })
        }
    }

    #[derive(Debug)]
    pub struct WriteError<E>(pub E);

    impl<E> From<E> for WriteError<E> {
        fn from(error: E) -> Self {
            Self(error)
        }
    }

    impl<E> Display for WriteError<E> {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            f.write_str("failed to write menu request")
        }
    }

    impl<E: 'static + Error> Error for WriteError<E> {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            Some(&self.0)
        }
    }

    #[derive(Debug)]
    #[non_exhaustive]
    pub enum ReadError<E> {
        Reading(E),
        CopyNonUtf8(CopyNonUtf8),
        UnknownRequestType(UnknownRequestType),
    }

    impl<E> From<E> for ReadError<E> {
        fn from(error: E) -> Self {
            Self::Reading(error)
        }
    }

    impl<E> Display for ReadError<E> {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            f.write_str("failed to read request by menu")
        }
    }

    impl<E: 'static + Error> Error for ReadError<E> {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            match self {
                Self::Reading(e) => Some(e),
                Self::CopyNonUtf8(e) => Some(e),
                Self::UnknownRequestType(e) => Some(e),
            }
        }
    }

    #[derive(Debug)]
    pub struct CopyNonUtf8(pub Utf8Error);

    impl Display for CopyNonUtf8 {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            f.write_str("requested to copy non-UTF8 data")
        }
    }

    impl Error for CopyNonUtf8 {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            Some(&self.0)
        }
    }

    #[derive(Debug)]
    pub struct UnknownRequestType(pub u8);

    impl Display for UnknownRequestType {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            write!(f, "unknown request type {}", self.0)
        }
    }

    impl Error for UnknownRequestType {}

    use crate::stream::Reader;
    use crate::stream::Writer;
    use std::error::Error;
    use std::fmt;
    use std::fmt::Display;
    use std::fmt::Formatter;
    use std::str::Utf8Error;
}
