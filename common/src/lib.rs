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
    #[derive(Clone, Copy)]
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
    #[derive(Clone, Copy)]
    pub enum Action {
        ShowList(List),
        Parent,
        Sync,
        Lock,
        LogOut,
        Navigate(Navigate),
    }

    #[derive(Clone, Copy)]
    pub enum Navigate {
        Back,
        Forward,
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
        Keybind {
            combination: "Alt+a",
            action: Action::ShowList(List::All),
            description: "All",
        },
        Keybind {
            combination: "Alt+t",
            action: Action::ShowList(List::Trash),
            description: "Trash",
        },
        Keybind {
            combination: "Alt+v",
            action: Action::ShowList(List::Favourites),
            description: "Favourites",
        },
        Keybind {
            combination: "Alt+d",
            action: Action::ShowList(List::Folders),
            description: "Folders",
        },
        Keybind {
            combination: "Alt+p",
            action: Action::Parent,
            description: "Parent",
        },
        Keybind {
            combination: "Alt+g",
            action: Action::ShowList(List::TypeBucket(CipherType::Login)),
            description: "Logins",
        },
        Keybind {
            combination: "Alt+n",
            action: Action::ShowList(List::TypeBucket(CipherType::SecureNote)),
            description: "Secure notes",
        },
        Keybind {
            combination: "Alt+c",
            action: Action::ShowList(List::TypeBucket(CipherType::Card)),
            description: "Cards",
        },
        Keybind {
            combination: "Alt+i",
            action: Action::ShowList(List::TypeBucket(CipherType::Identity)),
            description: "Identities",
        },
        Keybind {
            combination: "Alt+h",
            action: Action::Navigate(Navigate::Back),
            description: "Back",
        },
        Keybind {
            combination: "Alt+l",
            action: Action::Navigate(Navigate::Forward),
            description: "Forward",
        },
    ];

    /// Call the given callback with each row of keybinds to display.
    pub fn keybinds_ui<F: FnMut(&'static [Keybind<Action>]), HistoryItem: PartialEq>(
        history: Option<&History<HistoryItem>>,
        mut f: F,
    ) {
        f(&MENU_KEYBINDS[0..3]);
        // If the history isn’t `Some`, the menu hasn’t initialized.
        if let Some(history) = history {
            // Keybinds that select a category (e.g. all, trash) to be shown.
            f(&MENU_KEYBINDS[3..8]);
            // Keybinds that select a specific type bucket to be shown.
            f(&MENU_KEYBINDS[8..12]);
            // Back and forward keybinds
            match (history.can_go_back(), history.can_go_forward()) {
                (false, false) => {}
                (false, true) => f(&MENU_KEYBINDS[13..14]),
                (true, false) => f(&MENU_KEYBINDS[12..13]),
                (true, true) => f(&MENU_KEYBINDS[12..14]),
            }
        }
    }

    use crate::CipherType;
    use crate::Keybind;
    use crate::List;
    use rofi_bw_util::History;
}

pub use list::List;
mod list {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, bincode::Encode, bincode::Decode)]
    pub enum List {
        All,
        Trash,
        Favourites,
        TypeBucket(CipherType),
        Folders,
    }

    impl List {
        #[must_use]
        pub const fn description(self) -> &'static str {
            match self {
                Self::All => "All items",
                Self::Trash => "Trash",
                Self::Favourites => "Favourites",
                Self::TypeBucket(CipherType::Login) => "Logins",
                Self::TypeBucket(CipherType::SecureNote) => "Secure notes",
                Self::TypeBucket(CipherType::Card) => "Cards",
                Self::TypeBucket(CipherType::Identity) => "Identities",
                Self::Folders => "Folders",
            }
        }
    }

    use crate::CipherType;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub enum CipherType {
    Login = 0,
    SecureNote = 1,
    Card = 2,
    Identity = 3,
}
