/// All the information needed to create a `cairo::ImageSurface`.
/// Needed because `cairo::ImageSurface` itself is `!Send`.
pub(crate) struct CairoImageData {
    data: Box<[u8]>,
    format: cairo::Format,
    width: i32,
    height: i32,
    stride: i32,
}

impl CairoImageData {
    pub(crate) fn from_image<I>(image: &I) -> anyhow::Result<Self>
    where
        I: ?Sized + GenericImageView<Pixel = image::Rgba<u8>>,
    {
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

    pub(crate) fn into_surface(self) -> anyhow::Result<cairo::ImageSurface> {
        cairo::ImageSurface::create_for_data(
            self.data,
            self.format,
            self.width,
            self.height,
            self.stride,
        )
        .context("failed to create image surface")
    }
}

use anyhow::Context as _;
use image::GenericImageView;
use rofi_mode::cairo;
