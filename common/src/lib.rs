#![warn(
    clippy::pedantic,
    noop_method_call,
    trivial_casts,
    trivial_numeric_casts,
    unsafe_op_in_unsafe_fn,
    unused_lifetimes,
    unused_qualifications
)]
#![allow(clippy::missing_errors_doc, clippy::missing_panics_doc)]

pub use master_key::MasterKey;
mod master_key {
    #[derive(Clone)]
    pub struct MasterKey(pub Zeroizing<[u8; Self::LEN]>);

    impl MasterKey {
        pub const LEN: usize = 32;

        #[must_use]
        pub fn zeroed() -> Self {
            Self(Zeroizing::new([0; Self::LEN]))
        }
    }

    impl bincode::Encode for MasterKey {
        fn encode<E: bincode::enc::Encoder>(
            &self,
            encoder: &mut E,
        ) -> Result<(), bincode::error::EncodeError> {
            encoder.writer().write(&*self.0)
        }
    }

    impl bincode::Decode for MasterKey {
        fn decode<D: bincode::de::Decoder>(
            decoder: &mut D,
        ) -> Result<Self, bincode::error::DecodeError> {
            let mut this = Self::zeroed();
            decoder.reader().read(&mut *this.0)?;
            Ok(this)
        }
    }

    use bincode::de::read::Reader as _;
    use bincode::enc::write::Writer as _;
    use zeroize::Zeroizing;
}

pub mod ipc;

pub use keybinds::{Keybind, KEYBINDS};
pub mod keybinds {
    pub struct Keybind {
        pub combination: &'static str,
        pub action: Action,
        pub description: &'static str,
    }

    /// The keybindings, ordered by their custom command number.
    pub const KEYBINDS: &[Keybind] = &[
        Keybind {
            combination: "Control+s",
            action: Action::Sync,
            description: "Sync",
        },
        Keybind {
            combination: "Control+q",
            action: Action::Lock,
            description: "Lock",
        },
        Keybind {
            combination: "Control+o",
            action: Action::LogOut,
            description: "Log out",
        },
    ];

    pub enum Action {
        Sync,
        Lock,
        LogOut,
    }
}

pub mod fs;
