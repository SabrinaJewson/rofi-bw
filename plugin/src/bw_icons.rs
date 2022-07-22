pub(crate) struct BwIcons {
    icons: HashMap<Arc<str>, Icon>,
    disk_cache: Arc<DiskCache<fs::PathBuf>>,
    runtime: tokio::runtime::Runtime,
    http: reqwest::Client,
}

impl BwIcons {
    pub fn new() -> anyhow::Result<Self> {
        Self::new_inner().context("failed to initialize icons loader")
    }
    fn new_inner() -> anyhow::Result<Self> {
        let dirs = ProjectDirs::from("", "", "rofi-bw").context("no home directory")?;
        let disk_cache = DiskCache::new(dirs.cache_dir().join("icon-cache"));
        let runtime = tokio::runtime::Runtime::new().context("failed to start Tokio runtime")?;
        let http = reqwest::Client::builder()
            .build()
            .context("failed to initialize HTTP client")?;

        Ok(Self {
            icons: HashMap::new(),
            disk_cache: Arc::new(disk_cache),
            runtime,
            http,
        })
    }

    pub fn start_fetch(&mut self, host: Arc<str>) {
        if self.icons.contains_key(&*host) {
            return;
        }

        let handle = self.runtime.spawn_blocking({
            let disk_cache = self.disk_cache.clone();
            let host = host.clone();
            let http = self.http.clone();
            move || {
                if let Some(image) = disk_cache.load(&*host)? {
                    let image = fs::file::open::read_only(image)?;

                    let image = BufReader::new(image);

                    let image = image::io::Reader::new(image)
                        .with_guessed_format()
                        .context("failed to guess format")?
                        .decode()
                        .context("failed to decode image")?;

                    let image = rayon::scope(|_| CairoImageData::from_image(&image))?;

                    return Ok(Some(image));
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
                rayon::in_place_scope(|s| {
                    s.spawn(|_| {
                        cairo_image = Some((|| {
                            let image = image::load_from_memory(&*bytes)
                                .context("failed to decode image")?;

                            CairoImageData::from_image(&image)
                        })());
                    });

                    disk_cache.store(&*host, &*bytes, expires)
                })?;
                Ok(Some(cairo_image.unwrap()?))
            }
        });

        self.icons.insert(host, Icon::Waiting(handle));
    }

    pub fn get(&mut self, host: &str) -> Option<cairo::Surface> {
        let icon = self.icons.get_mut(host).unwrap();

        if let Icon::Waiting(handle) = icon {
            let task_result = poll_future_once(handle)?;

            let surface_result: anyhow::Result<_> = (|| {
                let image_data = match task_result.unwrap()? {
                    Some(image_data) => image_data,
                    None => return Ok(None),
                };

                let surface = cairo::ImageSurface::create_for_data(
                    image_data.data,
                    image_data.format,
                    image_data.width,
                    image_data.height,
                    image_data.stride,
                )
                .context("failed to create image surface")?;

                Ok(Some(surface))
            })();

            *icon = Icon::Complete(match surface_result {
                Ok(Some(surface)) => Some(SyncWrapper::new(surface)),
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
            Icon::Complete(surface) => surface.as_mut().map(|s| (**s.get_mut()).clone()),
        }
    }

    pub fn fs_path(&self, host: &str) -> Option<fs::PathBuf> {
        self.disk_cache.load(host).ok().flatten()
    }
}

enum Icon {
    Waiting(tokio::task::JoinHandle<anyhow::Result<Option<CairoImageData>>>),
    Complete(Option<SyncWrapper<cairo::ImageSurface>>),
}

struct CairoImageData {
    data: Box<[u8]>,
    format: cairo::Format,
    width: i32,
    height: i32,
    stride: i32,
}

impl CairoImageData {
    fn from_image(image: &DynamicImage) -> anyhow::Result<Self> {
        let stride = cairo::Format::ARgb32
            .stride_for_width(image.width())
            .context("failed to get Cairo stride")?;

        let stride_usize = usize::try_from(stride).context("invalid stride")?;
        let width_i32 = i32::try_from(image.width()).context("image too wide")?;
        let height_i32 = i32::try_from(image.height()).context("image too tall")?;

        let mut data = vec![0; stride_usize * image.height() as usize].into_boxed_slice();
        for (y, row) in data.chunks_exact_mut(stride_usize).enumerate() {
            let row = &mut row[..image.width() as usize * 4];

            for (x, pixel) in row.chunks_exact_mut(4).enumerate() {
                // these casts are OK because we know the image is < u32::MAX by u32::MAX
                #[allow(clippy::cast_possible_truncation)]
                let image::Rgba([r, g, b, a]) = image.get_pixel(x as u32, y as u32);
                let argb = u32::from_be_bytes([a, r, g, b]).to_ne_bytes();
                pixel.copy_from_slice(&argb);
            }
        }

        Ok(Self {
            data,
            format: cairo::Format::ARgb32,
            width: width_i32,
            height: height_i32,
            stride,
        })
    }
}

pub(crate) use download_icon::download_icon;
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

pub(crate) use sync_wrapper::SyncWrapper;
mod sync_wrapper {
    pub(crate) struct SyncWrapper<T>(T);

    impl<T> SyncWrapper<T> {
        pub(crate) const fn new(value: T) -> Self {
            Self(value)
        }

        pub(crate) fn get_mut(&mut self) -> &mut T {
            &mut self.0
        }
    }

    unsafe impl<T> Sync for SyncWrapper<T> {}
}

use poll_future_once::poll_future_once;
mod poll_future_once {
    pub(crate) fn poll_future_once<F: Future>(future: F) -> Option<F::Output> {
        pin!(future);
        let waker = NOOP_WAKER;
        let cx = &mut task::Context::from_waker(&waker);
        match future.poll(cx) {
            Poll::Ready(val) => Some(val),
            Poll::Pending => None,
        }
    }

    use super::NOOP_WAKER;
    use std::future::Future;
    use std::task;
    use std::task::Poll;
    use tokio::pin;
}

use noop_waker::NOOP_WAKER;
mod noop_waker {
    pub(crate) const NOOP_WAKER: Waker = unsafe { mem::transmute(RAW) };
    const RAW: RawWaker = RawWaker::new(ptr::null(), &VTABLE);
    const VTABLE: RawWakerVTable = RawWakerVTable::new(|_| RAW, |_| {}, |_| {}, |_| {});

    use std::mem;
    use std::ptr;
    use std::task::RawWaker;
    use std::task::RawWakerVTable;
    use std::task::Waker;
}

use crate::disk_cache::DiskCache;
use anyhow::Context as _;
use directories::ProjectDirs;
use image::DynamicImage;
use image::GenericImageView;
use rofi_bw_common::fs;
use rofi_mode::cairo;
use std::collections::HashMap;
use std::io::BufReader;
use std::sync::Arc;
