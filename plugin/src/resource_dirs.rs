//! This plugin needs to load some stuff from `/usr/local/share/rofi-bw` or `/usr/share/rofi-bw`.
//! This path also needs to be configurable, just for the sake of having `cargo dev run` work
//! without root permissions.

#[derive(Debug, Clone)]
pub(crate) enum ResourceDirs {
    Dynamic(Arc<OsStr>),
    Default,
}

impl ResourceDirs {
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

impl<'dirs> IntoIterator for &'dirs ResourceDirs {
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
        let dirs = ResourceDirs::Dynamic(Arc::from(OsStr::from_bytes(b"foo:bar:baz:")));
        let dirs = dirs
            .iter()
            .map(|path| path.as_os_str().as_bytes())
            .collect::<Vec<_>>();
        let expected: [&[u8]; 4] = [b"foo", b"bar", b"baz", b""];
        assert_eq!(dirs, expected,);
    }

    #[test]
    fn iter_default() {
        let dirs = ResourceDirs::Default
            .iter()
            .map(|path| path.as_os_str().as_bytes())
            .collect::<Vec<_>>();
        let expected: [&[u8]; 2] = [b"/usr/share/rofi-bw", b"/usr/local/share/rofi-bw"];
        assert_eq!(dirs, expected);
    }

    use super::ResourceDirs;
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
