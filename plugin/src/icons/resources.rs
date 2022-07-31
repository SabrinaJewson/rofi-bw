//! Icons loaded from `rofi-bw`’s resources directories (`/usr/local/share/rofi-bw` by default).
//! These are the bank card icons.
// TODO: reduce code duplciation between this and bitwarden.rs

pub(crate) struct Resources {
    dirs: Dirs,
    icons: [Option<IconState>; Resource::COUNT],
}

impl Resources {
    pub(crate) fn new() -> Self {
        // const to allow array repetition syntax
        const NO_STATE: Option<IconState> = None;

        Self {
            dirs: Dirs::from_env(),
            icons: [NO_STATE; Resource::COUNT],
        }
    }

    pub(crate) fn start_fetch(&mut self, icon: Resource) {
        if self.icons[icon as usize].is_some() {
            return;
        }

        let dirs = self.dirs.clone();
        let handle = tokio::task::spawn_blocking(move || {
            let file_name = icon.file_name();

            let mut file = None;
            for dir in &dirs {
                let path = dir.join(file_name);
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

        self.icons[icon as usize] = Some(IconState::Loading(handle));
    }

    pub(crate) fn surface(&mut self, icon: Resource) -> Option<cairo::Surface> {
        let icon = self.get(icon)?;
        Some((**icon.surface.get_mut()).clone())
    }

    pub(crate) fn fs_path(&mut self, icon: Resource) -> Option<&fs::Path> {
        let icon = self.get(icon)?;
        Some(&*icon.path)
    }

    fn get(&mut self, icon: Resource) -> Option<&mut LoadedIcon> {
        let icon_state = self.icons[icon as usize].as_mut().unwrap();

        if let IconState::Loading(handle) = icon_state {
            let task_result = poll_future_once(handle)?;

            let surface_result: anyhow::Result<_> = (|| {
                let (path, image_data) = task_result.unwrap()?;
                Ok((path, image_data.into_surface()?))
            })();

            *icon_state = IconState::Loaded(match surface_result {
                Ok((path, surface)) => Some(LoadedIcon {
                    path,
                    surface: SyncWrapper::new(surface),
                }),
                Err(e) => {
                    let context = format!("failed to load icon {}", icon.to_str());
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

use dirs::Dirs;
mod dirs {
    #[derive(Debug, Clone)]
    pub(crate) enum Dirs {
        Dynamic(Arc<OsStr>),
        Default,
    }

    impl Dirs {
        pub(crate) fn from_env() -> Self {
            match env::var_os("ROFI_BW_RESOURCES_DIR") {
                Some(env) => Self::Dynamic(Arc::from(&*env)),
                None => Self::Default,
            }
        }

        pub(crate) fn iter(&self) -> Iter<'_> {
            match self {
                Self::Dynamic(os_str) => Iter::Dynamic(os_str.as_bytes()),
                Self::Default => Iter::BeforeUsr,
            }
        }
    }

    impl<'dirs> IntoIterator for &'dirs Dirs {
        type Item = &'dirs fs::Path;
        type IntoIter = Iter<'dirs>;
        fn into_iter(self) -> Self::IntoIter {
            self.iter()
        }
    }

    pub(crate) enum Iter<'dirs> {
        Dynamic(&'dirs [u8]),
        BeforeUsr,
        BeforeUsrLocal,
        Finished,
    }

    impl<'dirs> Iterator for Iter<'dirs> {
        type Item = &'dirs fs::Path;

        fn next(&mut self) -> Option<Self::Item> {
            match &*self {
                &Self::Dynamic(remaining) => {
                    let bytes = if let Some(colon) = memchr(b':', remaining) {
                        *self = Self::Dynamic(&remaining[colon + 1..]);
                        &remaining[..colon]
                    } else {
                        *self = Self::Finished;
                        remaining
                    };
                    Some(fs::Path::new(OsStr::from_bytes(bytes)))
                }
                Self::BeforeUsr => {
                    *self = Self::BeforeUsrLocal;
                    Some(fs::Path::new("/usr/share/rofi-bw"))
                }
                Self::BeforeUsrLocal => {
                    *self = Self::Finished;
                    Some(fs::Path::new("/usr/local/share/rofi-bw"))
                }
                Self::Finished => None,
            }
        }
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn iter_dynamic() {
            let dirs = Dirs::Dynamic(Arc::from(OsStr::from_bytes(b"foo:bar:baz:")));
            let dirs = dirs
                .iter()
                .map(|path| path.as_os_str().as_bytes())
                .collect::<Vec<_>>();
            let expected: [&[u8]; 4] = [b"foo", b"bar", b"baz", b""];
            assert_eq!(dirs, expected,);
        }

        #[test]
        fn iter_default() {
            let dirs = Dirs::Default
                .iter()
                .map(|path| path.as_os_str().as_bytes())
                .collect::<Vec<_>>();
            let expected: [&[u8]; 2] = [b"/usr/share/rofi-bw", b"/usr/local/share/rofi-bw"];
            assert_eq!(dirs, expected);
        }

        use super::Dirs;
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt as _;
        use std::sync::Arc;
    }

    use memchr::memchr;
    use rofi_bw_common::fs;
    use std::env;
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt as _;
    use std::sync::Arc;
}

enum IconState {
    Loading(tokio::task::JoinHandle<anyhow::Result<(fs::PathBuf, CairoImageData)>>),
    Loaded(Option<LoadedIcon>),
}

struct LoadedIcon {
    path: fs::PathBuf,
    surface: SyncWrapper<cairo::ImageSurface>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Resource {
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

impl Resource {
    const COUNT: usize = 10;

    pub(crate) fn card_icon(s: &str) -> Option<Self> {
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

use crate::cairo_image_data::CairoImageData;
use crate::poll_future_once::poll_future_once;
use crate::SyncWrapper;
use anyhow::Context as _;
use rofi_bw_common::fs;
use rofi_mode::cairo;
use std::io;
use std::io::BufReader;
