pub(crate) struct Icons {
    bitwarden: Bitwarden,
}

impl Icons {
    pub(crate) fn new() -> anyhow::Result<Self> {
        Ok(Self {
            bitwarden: Bitwarden::new()?,
        })
    }

    pub(crate) fn start_fetch(&mut self, icon: Icon) {
        match icon {
            Icon::Host(host) => self.bitwarden.start_fetch(host),
        }
    }

    pub(crate) fn surface(&mut self, icon: &Icon) -> Option<cairo::Surface> {
        match icon {
            Icon::Host(host) => self.bitwarden.surface(host),
        }
    }

    pub(crate) fn fs_path(&mut self, icon: &Icon) -> Option<&fs::Path> {
        match icon {
            Icon::Host(host) => self.bitwarden.fs_path(host),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum Icon {
    Host(Arc<str>),
}

use bitwarden::Bitwarden;
mod bitwarden;

use rofi_bw_common::fs;
use rofi_mode::cairo;
use std::sync::Arc;
