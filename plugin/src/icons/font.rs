//! Icons loaded from the file `bwi-font.ttf`.

pub(crate) struct Font {
    inner: Inner,
    cache: [Option<CachedSurface>; Glyph::COUNT],
}

impl Font {
    pub(crate) fn new(data_dirs: &fs::path::List) -> anyhow::Result<Self> {
        let library = freetype::Library::init().context("failed to initialize FreeType")?;

        let mut freetype_face = None;
        for data_dir in data_dirs {
            let path = data_dir.join("rofi-bw/bwi-font.ttf");
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
            inner: Inner {
                freetype_face,
                cairo_face,
            },
            cache: Default::default(),
        })
    }

    pub(crate) fn surface(&mut self, glyph: Glyph, height: u32) -> Option<cairo::Surface> {
        let cache_entry = &mut self.cache[glyph as usize];

        if let Some(cached) = cache_entry {
            let cached = match cached {
                CachedSurface::Surface(surface) => surface,
                CachedSurface::Errored => return None,
            };
            if u32::try_from(cached.height()).map_or(false, |h| h >= height) {
                return Some((**cached).clone());
            }
        }

        let (ret, for_cache) = match self.inner.create_icon(glyph, height) {
            Ok(icon) => (Some((*icon).clone()), CachedSurface::Surface(icon)),
            Err(e) => {
                let context = format!("failed to create icon for glyph {glyph:?}");
                eprintln!("Warning: {:?}", e.context(context));
                (None, CachedSurface::Errored)
            }
        };

        *cache_entry = Some(for_cache);

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
        // Setting the full height can cause a bit of clipping on some of the icons (I especially
        // noticed this on the key one), so it’s more reliable to make it slightly smaller.
        context.set_font_size(f64::from(height) * 0.95);

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

    Folder,

    Briefcase,
    Chain,
    Clock,
    EyeSlash,
    Hash,
    Key,
    List,
    Mail,
    Mobile,
    Padlock,
    Pencil,
    Square,
    SquareCheck,
    User,
}

impl Glyph {
    const COUNT: usize = 19;

    fn to_char(self) -> u16 {
        // See:
        // https://github.com/bitwarden/clients/blob/8820a42ec9d0d0919f01b3d88b422abe0569c923/libs/angular/src/scss/bwicons/styles/style.scss#L102
        match self {
            Self::Login => 0xE909,
            Self::SecureNote => 0xE90A,
            Self::Card => 0xE908,
            Self::Identity => 0xE907,

            Self::Folder => 0xE90B,

            Self::Briefcase => 0xE98C,
            Self::Chain => 0xE954,
            Self::Clock => 0xE92C,
            Self::EyeSlash => 0xE96D,
            Self::Hash => 0xE904,
            Self::Key => 0xE902,
            Self::List => 0xE91A,
            Self::Mail => 0xE949,
            Self::Mobile => 0xE986,
            Self::Padlock => 0xE90C,
            Self::Pencil => 0xE929,
            Self::Square => 0xE92F,
            Self::SquareCheck => 0xE93B,
            Self::User => 0xE900,
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

use anyhow::Context as _;
use rofi_bw_util::fs;
use rofi_mode::cairo;
