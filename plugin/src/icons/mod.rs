// TODO: SyncWrapper everything
pub(crate) struct Icons {
    bitwarden: Bitwarden,
    cards: Cards,
    font: Font,
    runtime: tokio::runtime::Runtime,
    resource_dirs: ResourceDirs,
}

impl Icons {
    pub(crate) fn new() -> anyhow::Result<Self> {
        let resource_dirs = ResourceDirs::from_env();
        Ok(Self {
            bitwarden: Bitwarden::new()?,
            cards: Cards::new(),
            font: Font::new(&resource_dirs)?,
            runtime: tokio::runtime::Runtime::new().context("failed to start Tokio runtime")?,
            resource_dirs,
        })
    }

    pub(crate) fn start_fetch(&mut self, icon: &Icon) {
        let _runtime_context = self.runtime.enter();
        match icon {
            Icon::Host(host) => self.bitwarden.start_fetch(host.clone()),
            &Icon::Card(card) => self.cards.start_fetch(&self.resource_dirs, card),
            Icon::Glyph(_) => {}
        }
    }

    pub(crate) fn surface(&mut self, icon: &Icon, height: u32) -> Option<cairo::Surface> {
        match icon {
            Icon::Host(host) => self.bitwarden.surface(host),
            &Icon::Card(card) => self.cards.surface(card),
            &Icon::Glyph(glyph) => self.font.surface(glyph, height),
        }
    }

    pub(crate) fn fs_path(&mut self, icon: &Icon) -> Option<&fs::Path> {
        match icon {
            Icon::Host(host) => self.bitwarden.fs_path(host),
            &Icon::Card(card) => self.cards.fs_path(card),
            &Icon::Glyph(_) => None,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum Icon {
    Host(Arc<str>),
    Card(Card),
    Glyph(Glyph),
}

impl Icon {
    pub(crate) fn card(brand: &str) -> Option<Self> {
        Card::from_str(brand).map(Self::Card)
    }
}

use bitwarden::Bitwarden;
mod bitwarden;

pub(crate) use cards::Card;
use cards::Cards;
mod cards;

use font::Font;
pub(crate) use font::Glyph;
mod font;

use crate::resource_dirs::ResourceDirs;
use anyhow::Context as _;
use rofi_bw_common::fs;
use rofi_mode::cairo;
use std::sync::Arc;
