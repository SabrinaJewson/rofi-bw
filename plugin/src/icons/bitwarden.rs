//! Per-website icons downloaded and cached from icons.bitwarden.net.

pub(crate) struct Bitwarden {
    icons: HashMap<Arc<str>, Icon>,
    disk_cache: Arc<DiskCache<fs::PathBuf>>,
    http: reqwest::Client,
}

impl Bitwarden {
    pub(crate) fn new() -> anyhow::Result<Self> {
        Self::new_inner().context("failed to initialize Bitwarden icons loader")
    }
    fn new_inner() -> anyhow::Result<Self> {
        let dirs = ProjectDirs::from("", "", "rofi-bw").context("no home directory")?;
        let disk_cache = DiskCache::new(dirs.cache_dir().join("icon-cache"));
        let http = reqwest::Client::builder()
            .build()
            .context("failed to initialize HTTP client")?;

        Ok(Self {
            icons: HashMap::new(),
            disk_cache: Arc::new(disk_cache),
            http,
        })
    }

    pub(crate) fn start_fetch(&mut self, host: Arc<str>) {
        if self.icons.contains_key(&*host) {
            return;
        }

        let handle = tokio::task::spawn_blocking({
            let disk_cache = self.disk_cache.clone();
            let host = host.clone();
            let http = self.http.clone();
            move || {
                if let Some(image_file) = disk_cache.load(&*host)? {
                    let image_file = fs::file::open::read_only(image_file)?;

                    let mut image_file = BufReader::new(image_file);

                    let image = image::io::Reader::new(&mut image_file)
                        .with_guessed_format()
                        .context("failed to guess format")?
                        .decode()
                        .context("failed to decode image")?;

                    let image = rayon::scope(|_| CairoImageData::from_image(&image))?;

                    return Ok(Some((image_file.into_inner().into_path(), image)));
                }

                let runtime = tokio::runtime::Handle::current();
                let handle = tokio::spawn(async move {
                    let icon = download_icon(&http, &*host).await?;
                    anyhow::Ok((icon, host))
                });
                let (download_icon::Downloaded { bytes, expires }, host) =
                    match runtime.block_on(handle) {
                        Ok(res) => res?,
                        Err(e) if e.is_cancelled() => return Ok(None),
                        Err(e) => panic!("inner task panicked: {e:?}"),
                    };

                let mut cairo_image: Option<anyhow::Result<_>> = None;
                let path = rayon::in_place_scope(|s| {
                    s.spawn(|_| {
                        cairo_image = Some((|| {
                            let image = image::load_from_memory(&*bytes)
                                .context("failed to decode image")?;

                            CairoImageData::from_image(&image)
                        })());
                    });

                    disk_cache.store(&*host, &*bytes, expires)
                })?;
                Ok(Some((path, cairo_image.unwrap()?)))
            }
        });

        self.icons.insert(host, Icon::Waiting(handle));
    }

    pub(crate) fn surface(&mut self, host: &str) -> Option<cairo::Surface> {
        let icon = self.get(host)?;
        Some((*icon.surface).clone())
    }

    pub(crate) fn fs_path(&mut self, host: &str) -> Option<&fs::Path> {
        let icon = self.get(host)?;
        Some(&*icon.path)
    }

    fn get(&mut self, host: &str) -> Option<&mut LoadedIcon> {
        let icon = self.icons.get_mut(host).unwrap();

        if let Icon::Waiting(handle) = icon {
            let task_result = poll_future_once(handle)?;

            let surface_result: anyhow::Result<_> = (|| {
                let (path, image_data) = match task_result.unwrap()? {
                    Some(data) => data,
                    None => return Ok(None),
                };

                let surface = image_data.into_surface()?;

                Ok(Some((path, surface)))
            })();

            *icon = Icon::Complete(match surface_result {
                Ok(Some((path, surface))) => Some(LoadedIcon { path, surface }),
                Ok(None) => None,
                Err(e) => {
                    let context = format!("failed to retrieve icon {host}");
                    eprintln!("Warning: {:?}", e.context(context));
                    None
                }
            });
        }

        match icon {
            Icon::Waiting(_) => unreachable!(),
            Icon::Complete(icon) => icon.as_mut(),
        }
    }
}

enum Icon {
    Waiting(tokio::task::JoinHandle<anyhow::Result<Option<(fs::PathBuf, CairoImageData)>>>),
    Complete(Option<LoadedIcon>),
}

struct LoadedIcon {
    path: fs::PathBuf,
    surface: cairo::ImageSurface,
}

use download_icon::download_icon;
mod download_icon {
    pub(crate) struct Downloaded {
        pub(crate) bytes: Bytes,
        pub(crate) expires: SystemTime,
    }

    pub(crate) async fn download_icon(
        client: &reqwest::Client,
        host: &str,
    ) -> anyhow::Result<Downloaded> {
        inner(client, host)
            .await
            .with_context(|| format!("failed to download icon {host}"))
    }
    async fn inner(client: &reqwest::Client, host: &str) -> anyhow::Result<Downloaded> {
        let response = client
            .get(format!("https://icons.bitwarden.net/{host}/icon.png"))
            .send()
            .await
            .context("failed to send request")?
            .error_for_status()?;

        let expires = (|| {
            let header = response.headers().get("expires")?;
            let header = header.to_str().ok()?;
            OffsetDateTime::parse(header, &time::format_description::well_known::Rfc2822).ok()
        })();

        let expires = SystemTime::from(expires.unwrap_or_else(default_expires));

        let bytes = response
            .bytes()
            .await
            .context("failed to read response body")?;

        Ok(Downloaded { bytes, expires })
    }

    fn default_expires() -> OffsetDateTime {
        // about a month is a good default expiry time, it’s what Bitwarden’s server seems to use.
        let month = time::Duration::seconds(60 * 60 * 24 * 30);
        OffsetDateTime::now_utc().saturating_add(month)
    }

    use anyhow::Context as _;
    use bytes::Bytes;
    use std::time::SystemTime;
    use time::OffsetDateTime;
}

use crate::poll_future_once;
use crate::CairoImageData;
use crate::DiskCache;
use anyhow::Context as _;
use directories::ProjectDirs;
use rofi_bw_util::fs;
use rofi_mode::cairo;
use std::collections::HashMap;
use std::io::BufReader;
use std::sync::Arc;
