//! Icons loaded from the file `bwi-font.ttf`.

pub(crate) struct Font {
    inner: SyncWrapper<Inner>,
    cache: SyncWrapper<[Option<CachedSurface>; Glyph::COUNT]>,
}

impl Font {
    pub(crate) fn new(resource_dirs: &ResourceDirs) -> anyhow::Result<Self> {
        let library = freetype::Library::init().context("failed to initialize FreeType")?;

        let mut freetype_face = None;
        for resource_dir in resource_dirs {
            let path = resource_dir.join("bwi-font.ttf");
            match library.new_face(&*path, 0) {
                Ok(loaded_face) => {
                    freetype_face = Some(loaded_face);
                    break;
                }
                Err(freetype::Error::CannotOpenResource) => continue,
                Err(e) => {
                    return Err(e).context(format!("failed to open font file {}", path.display()))
                }
            }
        }
        let freetype_face = freetype_face.context("font file bwi-font.ttf not found")?;

        let cairo_face = cairo_font_face_from_freetype(freetype_face.clone())?;

        Ok(Self {
            inner: SyncWrapper::new(Inner {
                freetype_face,
                cairo_face,
            }),
            cache: SyncWrapper::default(),
        })
    }

    pub(crate) fn surface(&mut self, glyph: Glyph, height: u32) -> Option<cairo::Surface> {
        let cache = self.cache.get_mut();
        if let Some(cached) = &cache[glyph as usize] {
            let cached = match cached {
                CachedSurface::Surface(surface) => surface,
                CachedSurface::Errored => return None,
            };
            if u32::try_from(cached.height()).map_or(false, |h| h >= height) {
                return Some((**cached).clone());
            }
        }

        let (ret, for_cache) = match self.inner.get_mut().create_icon(glyph, height) {
            Ok(icon) => (Some((*icon).clone()), CachedSurface::Surface(icon)),
            Err(e) => {
                let context = format!("failed to create icon for glyph {glyph:?}");
                eprintln!("Warning: {:?}", e.context(context));
                (None, CachedSurface::Errored)
            }
        };

        cache[glyph as usize] = Some(for_cache);

        ret
    }
}

enum CachedSurface {
    Errored,
    Surface(cairo::ImageSurface),
}

struct Inner {
    freetype_face: freetype::Face,
    cairo_face: cairo::FontFace,
}

impl Inner {
    fn create_icon(&mut self, glyph: Glyph, height: u32) -> anyhow::Result<cairo::ImageSurface> {
        let height_signed = i32::try_from(height).unwrap();

        let index = self
            .freetype_face
            .get_char_index(usize::from(glyph.to_char()));

        let width = height_signed;

        let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, width, height_signed)
            .context("failed to create image surface")?;

        let context = cairo::Context::new(&*surface).context("failed to create Cairo context")?;

        context.set_font_face(&self.cairo_face);
        context.set_source_rgb(0.7, 0.7, 0.7);
        context.set_font_size(f64::from(height));

        let mut glyphs = [cairo::Glyph {
            index: u64::from(index),
            x: 0.0,
            y: 0.0,
        }];

        let extents = context
            .glyph_extents(&glyphs)
            .context("failed to get glyph extents")?;

        // Center the glyph
        glyphs[0].x = f64::from(width) / 2.0 - (extents.width / 2.0 + extents.x_bearing);
        glyphs[0].y = f64::from(height) / 2.0 - (extents.height / 2.0 + extents.y_bearing);

        context
            .show_glyphs(&glyphs)
            .context("failed to draw glyph")?;

        Ok(surface)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Glyph {
    Login = 0,
    SecureNote,
    Card,
    Identity,
}

impl Glyph {
    const COUNT: usize = 4;

    fn to_char(self) -> u16 {
        // See:
        // https://github.com/bitwarden/clients/blob/8820a42ec9d0d0919f01b3d88b422abe0569c923/libs/angular/src/scss/bwicons/styles/style.scss#L102
        match self {
            Self::Login => 0xE909,
            Self::SecureNote => 0xE90A,
            Self::Card => 0xE908,
            Self::Identity => 0xE907,
        }
    }
}

use cairo_font_face_from_freetype::cairo_font_face_from_freetype;
mod cairo_font_face_from_freetype {
    pub(crate) fn cairo_font_face_from_freetype(
        mut freetype: freetype::Face,
    ) -> anyhow::Result<cairo::FontFace> {
        let cairo = unsafe {
            cairo::FontFace::from_raw_full(cairo::ffi::cairo_ft_font_face_create_for_ft_face(
                <*mut _>::cast(freetype.raw_mut()),
                0,
            ))
        };

        static KEY: cairo::UserDataKey<freetype::Face> = cairo::UserDataKey::new();
        cairo
            .set_user_data(&KEY, Rc::new(freetype))
            .context("failed to set font face user data")?;

        cairo.status().context("loading font face failed")?;

        Ok(cairo)
    }

    use anyhow::Context as _;
    use rofi_mode::cairo;
    use std::rc::Rc;
}

use crate::resource_dirs::ResourceDirs;
use crate::SyncWrapper;
use anyhow::Context as _;
use rofi_mode::cairo;
