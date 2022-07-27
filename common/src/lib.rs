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

    impl PartialEq for MasterKey {
        fn eq(&self, other: &Self) -> bool {
            // constant-time equality, just to be safe
            self.ct_eq(other).into()
        }
    }

    impl ConstantTimeEq for MasterKey {
        fn ct_eq(&self, other: &Self) -> subtle::Choice {
            self.0.ct_eq(&*other.0)
        }
    }

    use bincode::de::read::Reader as _;
    use bincode::enc::write::Writer as _;
    use subtle::ConstantTimeEq;
    use zeroize::Zeroizing;
}

pub mod ipc;

pub use keybind::Keybind;
pub mod keybind {
    pub struct Keybind<Action> {
        pub combination: &'static str,
        pub action: Action,
        pub description: &'static str,
    }

    pub struct HelpMarkup<'keybinds, Action>(pub &'keybinds [Keybind<Action>]);

    impl<Action> Display for HelpMarkup<'_, Action> {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            for (i, keybind) in self.0.iter().enumerate() {
                if i != 0 {
                    f.write_str(" | ")?;
                }
                write!(f, "<b>{}</b>: {}", keybind.combination, keybind.description)?;
            }
            Ok(())
        }
    }

    pub fn apply_to_command<Action>(command: &mut process::Command, keybinds: &[Keybind<Action>]) {
        assert!(keybinds.len() <= 19);

        let mut arg_name_buf = String::new();
        for (i, keybind) in keybinds.iter().enumerate() {
            arg_name_buf.clear();
            write!(arg_name_buf, "-kb-custom-{}", i + 1).unwrap();
            command.arg(&*arg_name_buf).arg(keybind.combination);
        }
    }

    use std::fmt;
    use std::fmt::Display;
    use std::fmt::Formatter;
    use std::fmt::Write as _;
    use std::process;
}

pub use menu_keybinds::MENU_KEYBINDS;
pub mod menu_keybinds {
    pub enum Action {
        Sync,
        Lock,
        LogOut,
    }

    /// The keybindings, ordered by their custom command number.
    pub const MENU_KEYBINDS: &[Keybind<Action>] = &[
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

    use crate::Keybind;
}

pub mod fs;
