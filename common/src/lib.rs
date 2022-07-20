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

    use zeroize::Zeroizing;
}

pub mod ipc;

pub mod stream;

pub use keybinds::{Keybind, KEYBINDS};
pub mod keybinds {
    pub struct Keybind {
        pub combination: &'static str,
        pub action: MenuRequest<&'static str>,
        pub description: &'static str,
    }

    /// The keybindings, ordered by their custom command number.
    pub const KEYBINDS: &[Keybind] = &[
        Keybind {
            combination: "Control+s",
            action: MenuRequest::Sync,
            description: "Sync",
        },
        Keybind {
            combination: "Control+q",
            action: MenuRequest::Lock,
            description: "Lock",
        },
        Keybind {
            combination: "Control+o",
            action: MenuRequest::LogOut,
            description: "Log out",
        },
    ];

    use crate::ipc::MenuRequest;
}
