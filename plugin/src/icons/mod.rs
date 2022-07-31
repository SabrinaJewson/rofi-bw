pub(crate) struct Icons {
    runtime: tokio::runtime::Runtime,
    bitwarden: Bitwarden,
    resources: Resources,
}

impl Icons {
    pub(crate) fn new() -> anyhow::Result<Self> {
        Ok(Self {
            runtime: tokio::runtime::Runtime::new().context("failed to start Tokio runtime")?,
            bitwarden: Bitwarden::new()?,
            resources: Resources::new(),
        })
    }

    pub(crate) fn start_fetch(&mut self, icon: Icon) {
        let _runtime_context = self.runtime.enter();
        match icon {
            Icon::Host(host) => self.bitwarden.start_fetch(host),
            Icon::Resource(resource) => self.resources.start_fetch(resource),
        }
    }

    pub(crate) fn surface(&mut self, icon: &Icon) -> Option<cairo::Surface> {
        match icon {
            Icon::Host(host) => self.bitwarden.surface(host),
            &Icon::Resource(resource) => self.resources.surface(resource),
        }
    }

    pub(crate) fn fs_path(&mut self, icon: &Icon) -> Option<&fs::Path> {
        match icon {
            Icon::Host(host) => self.bitwarden.fs_path(host),
            &Icon::Resource(resource) => self.resources.fs_path(resource),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum Icon {
    Host(Arc<str>),
    Resource(Resource),
}

use bitwarden::Bitwarden;
mod bitwarden;

pub(crate) use resources::Resource;
use resources::Resources;
mod resources;

use anyhow::Context as _;
use rofi_bw_common::fs;
use rofi_mode::cairo;
use std::sync::Arc;
