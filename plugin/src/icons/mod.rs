// Lots of the types used in this module end up `!Sync` since they use Cairo which is all `!Sync`.
// It’s easier to deal with this at the top level.
pub(crate) struct Icons(SyncWrapper<Inner>);

struct Inner {
    bitwarden: Bitwarden,
    cards: Cards,
    font: Font,
    runtime: tokio::runtime::Runtime,
    resource_dirs: Arc<fs::path::List>,
}

impl Icons {
    pub(crate) fn new() -> anyhow::Result<Self> {
        let resource_dirs = match fs::path::List::from_env_var("ROFI_BW_RESOURCES_DIR") {
            Some(dynamic) => dynamic.to_arc(),
            None => fs::path::List::from_ref("/usr/local/share/:/usr/share/").to_arc(),
        };

        Ok(Self(SyncWrapper::new(Inner {
            bitwarden: Bitwarden::new()?,
            cards: Cards::new(),
            font: Font::new(&resource_dirs)?,
            runtime: tokio::runtime::Runtime::new().context("failed to start Tokio runtime")?,
            resource_dirs,
        })))
    }

    pub(crate) fn start_fetch(&mut self, icon: &Icon) {
        let this = self.0.get_mut();
        let _runtime_context = this.runtime.enter();
        match icon {
            Icon::Host(host) => this.bitwarden.start_fetch(host.clone()),
            &Icon::Card(card) => this.cards.start_fetch(&this.resource_dirs, card),
            Icon::Glyph(_) => {}
        }
    }

    pub(crate) fn surface(&mut self, icon: &Icon, height: u32) -> Option<cairo::Surface> {
        let this = self.0.get_mut();
        match icon {
            Icon::Host(host) => this.bitwarden.surface(host),
            &Icon::Card(card) => this.cards.surface(card),
            &Icon::Glyph(glyph) => this.font.surface(glyph, height),
        }
    }

    pub(crate) fn fs_path(&mut self, icon: &Icon) -> Option<&fs::Path> {
        let this = self.0.get_mut();
        match icon {
            Icon::Host(host) => this.bitwarden.fs_path(host),
            &Icon::Card(card) => this.cards.fs_path(card),
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

impl From<Card> for Icon {
    fn from(card: Card) -> Self {
        Self::Card(card)
    }
}

impl From<Glyph> for Icon {
    fn from(glyph: Glyph) -> Self {
        Self::Glyph(glyph)
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

use crate::SyncWrapper;
use anyhow::Context as _;
use rofi_bw_common::fs;
use rofi_mode::cairo;
use std::sync::Arc;
