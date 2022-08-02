//! Card icons loaded from `rofi-bw`’s resources directories (`/usr/local/share/rofi-bw` by
//! default).
// TODO: reduce code duplciation between this and bitwarden.rs

pub(crate) struct Cards {
    icons: [Option<IconState>; Card::COUNT],
}

impl Cards {
    pub(crate) fn new() -> Self {
        // const to allow array repetition syntax
        const NO_STATE: Option<IconState> = None;

        Self {
            icons: [NO_STATE; Card::COUNT],
        }
    }

    pub(crate) fn start_fetch(&mut self, data_dirs: &Arc<fs::path::List>, card: Card) {
        if self.icons[card as usize].is_some() {
            return;
        }

        let data_dirs = data_dirs.clone();
        let handle = tokio::task::spawn_blocking(move || {
            let file_name = card.file_name();

            let mut file = None;
            for data_dir in &*data_dirs {
                let mut path = data_dir.join("rofi-bw/cards");
                path.push(file_name);
                match fs::file::open::read_only(path) {
                    Ok(opened_file) => {
                        file = Some(opened_file);
                        break;
                    }
                    Err(e) if e.source.kind() == io::ErrorKind::NotFound => continue,
                    Err(e) => return Err(e.into()),
                }
            }

            let file = file.context(format!("failed to locate icon {file_name}"))?;
            let mut file = BufReader::new(file);

            let image = image::io::Reader::new(&mut file)
                .with_guessed_format()
                .context("failed to guess format")?
                .decode()
                .context("failed to decode image")?;

            let image_data = rayon::scope(|_| CairoImageData::from_image(&image))?;

            Ok((file.into_inner().into_path(), image_data))
        });

        self.icons[card as usize] = Some(IconState::Loading(handle));
    }

    pub(crate) fn surface(&mut self, card: Card) -> Option<cairo::Surface> {
        let icon = self.get(card)?;
        Some((*icon.surface).clone())
    }

    pub(crate) fn fs_path(&mut self, card: Card) -> Option<&fs::Path> {
        let icon = self.get(card)?;
        Some(&*icon.path)
    }

    fn get(&mut self, card: Card) -> Option<&mut LoadedIcon> {
        let icon_state = self.icons[card as usize].as_mut().unwrap();

        if let IconState::Loading(handle) = icon_state {
            let task_result = poll_future_once(handle)?;

            let surface_result: anyhow::Result<_> = (|| {
                let (path, image_data) = task_result.unwrap()?;
                Ok((path, image_data.into_surface()?))
            })();

            *icon_state = IconState::Loaded(match surface_result {
                Ok((path, surface)) => Some(LoadedIcon { path, surface }),
                Err(e) => {
                    let context = format!("failed to load icon {}", card.to_str());
                    eprintln!("Warning: {:?}", e.context(context));
                    None
                }
            });
        }

        match icon_state {
            IconState::Loading(_) => unreachable!(),
            IconState::Loaded(icon) => icon.as_mut(),
        }
    }
}

enum IconState {
    Loading(tokio::task::JoinHandle<anyhow::Result<(fs::PathBuf, CairoImageData)>>),
    Loaded(Option<LoadedIcon>),
}

struct LoadedIcon {
    path: fs::PathBuf,
    surface: cairo::ImageSurface,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Card {
    Amex = 0,
    DinersClub,
    Discover,
    Jcb,
    Maestro,
    Mastercard,
    Mir,
    RuPay,
    UnionPay,
    Visa,
}

impl Card {
    const COUNT: usize = 10;

    pub(crate) fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "Amex" => Self::Amex,
            "Diners Club" => Self::DinersClub,
            "Discover" => Self::Discover,
            "JCB" => Self::Jcb,
            "Maestro" => Self::Maestro,
            "Mastercard" => Self::Mastercard,
            "Mir" => Self::Mir,
            "RuPay" => Self::RuPay,
            "UnionPay" => Self::UnionPay,
            "Visa" => Self::Visa,
            _ => return None,
        })
    }

    fn to_str(self) -> &'static str {
        match self {
            Self::Amex => "Amex",
            Self::DinersClub => "Diners Club",
            Self::Discover => "Discover",
            Self::Jcb => "JCB",
            Self::Maestro => "Maestro",
            Self::Mastercard => "Mastercard",
            Self::Mir => "Mir",
            Self::RuPay => "RuPay",
            Self::UnionPay => "UnionPay",
            Self::Visa => "Visa",
        }
    }

    fn file_name(self) -> &'static str {
        // TODO: Allow light icons as well?
        match self {
            Self::Amex => "amex-dark.png",
            Self::DinersClub => "diners_club-dark.png",
            Self::Discover => "discover-dark.png",
            Self::Jcb => "jcb-dark.png",
            Self::Maestro => "maestro-dark.png",
            Self::Mastercard => "mastercard-dark.png",
            Self::Mir => "mir-dark.png",
            Self::RuPay => "ru_pay-dark.png",
            Self::UnionPay => "union_pay-dark.png",
            Self::Visa => "visa-dark.png",
        }
    }
}

use crate::poll_future_once;
use crate::CairoImageData;
use anyhow::Context as _;
use rofi_bw_util::fs;
use rofi_mode::cairo;
use std::io;
use std::io::BufReader;
use std::sync::Arc;
