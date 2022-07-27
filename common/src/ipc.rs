/// The name of the environment variable used to pass the FD of the communication pipe
/// to the child process.
pub const PIPE_FD_ENV_VAR: &str = "ROFI_BW_PIPE_FD";

pub use handshake::Handshake;
pub mod handshake {
    #[derive(Clone, Copy, bincode::Encode, bincode::Decode)]
    pub struct Handshake<MasterKeyT, DataT> {
        pub master_key: MasterKeyT,
        pub data: DataT,
    }

    pub fn write<W, MasterKeyT, DataT>(
        mut writer: W,
        handshake: &Handshake<MasterKeyT, DataT>,
    ) -> Result<(), WriteError>
    where
        W: io::Write,
        MasterKeyT: Borrow<MasterKey> + bincode::Encode,
        DataT: Borrow<[u8]> + bincode::Encode,
    {
        let config = bincode::config::standard();
        bincode::encode_into_std_write(handshake, &mut writer, config).map_err(WriteError)?;
        Ok(())
    }

    #[derive(Debug)]
    pub struct WriteError(bincode::error::EncodeError);

    impl Display for WriteError {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            f.write_str("failed to write handshake")
        }
    }

    impl Error for WriteError {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            Some(&self.0)
        }
    }

    pub fn read<R: io::BufRead>(
        mut reader: R,
    ) -> Result<Handshake<MasterKey, Box<[u8]>>, ReadError> {
        let config = bincode::config::standard();
        bincode::decode_from_std_read(&mut reader, config).map_err(ReadError)
    }

    #[derive(Debug)]
    pub struct ReadError(bincode::error::DecodeError);

    impl Display for ReadError {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            f.write_str("failed to read handshake")
        }
    }

    impl Error for ReadError {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            Some(&self.0)
        }
    }

    use crate::MasterKey;
    use std::borrow::Borrow;
    use std::error::Error;
    use std::fmt;
    use std::fmt::Display;
    use std::fmt::Formatter;
    use std::io;
}

pub use menu_request::MenuRequest;
/// A request from the menu to the parent process.
pub mod menu_request {
    // Initially I tried to make this type generic, but I’ve since given up since it’s just too
    // much work carrying around 5 generic parameters everywhere and dealing with type inference
    // and type unification errors.
    #[derive(Debug, Clone, bincode::Encode, bincode::Decode)]
    pub enum MenuRequest {
        Copy {
            /// Used in notifications and for the reprompt message
            name: String,
            data: String,
            /// Used in notifications
            image_path: Option<String>,
            reprompt: bool,
            /// When a reprompt is cancelled the old menu state should be restored.
            menu_state: MenuState,
        },
        Sync {
            menu_state: MenuState,
        },
        Lock,
        LogOut,
        Exit,
    }

    pub fn write<W>(mut writer: W, menu_request: &MenuRequest) -> Result<(), WriteError>
    where
        W: io::Write,
    {
        let config = bincode::config::standard();
        bincode::encode_into_std_write(menu_request, &mut writer, config).map_err(WriteError)?;
        Ok(())
    }

    #[derive(Debug)]
    pub struct WriteError(bincode::error::EncodeError);

    impl Display for WriteError {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            f.write_str("failed to write menu request")
        }
    }

    impl Error for WriteError {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            Some(&self.0)
        }
    }

    pub fn read<R: io::BufRead>(mut reader: R) -> Result<MenuRequest, ReadError> {
        let config = bincode::config::standard();
        bincode::decode_from_std_read(&mut reader, config).map_err(ReadError)
    }

    #[derive(Debug)]
    pub struct ReadError(bincode::error::DecodeError);

    impl Display for ReadError {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            f.write_str("failed to read menu request")
        }
    }

    impl Error for ReadError {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            Some(&self.0)
        }
    }

    /// Old state of the menu that can be restored.
    #[derive(Debug, Clone, bincode::Encode, bincode::Decode)]
    pub struct MenuState {
        pub filter: String,
    }

    use std::error::Error;
    use std::fmt;
    use std::fmt::Display;
    use std::fmt::Formatter;
    use std::io;
}
