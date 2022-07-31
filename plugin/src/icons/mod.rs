pub(crate) struct Icons {
    bitwarden: Bitwarden,
    runtime: tokio::runtime::Runtime,
}

impl Icons {
    pub(crate) fn new() -> anyhow::Result<Self> {
        Ok(Self {
            bitwarden: Bitwarden::new()?,
            runtime: tokio::runtime::Runtime::new().context("failed to start Tokio runtime")?,
        })
    }

    pub(crate) fn start_fetch(&mut self, icon: Icon) {
        let _runtime_context = self.runtime.enter();
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

use anyhow::Context as _;
use rofi_bw_common::fs;
use rofi_mode::cairo;
use std::sync::Arc;
