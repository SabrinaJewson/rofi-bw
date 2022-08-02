//! `std::fs` if it was good

pub use path::Path;
pub use path::PathBuf;
pub mod path {
    #![allow(clippy::module_name_repetitions)]

    pub use std::path::Path;
    pub use std::path::PathBuf;

    pub use parent::parent;
    pub mod parent {
        pub fn parent(path: &fs::Path) -> Result<&fs::Path, Error> {
            path.parent()
                .ok_or_else(|| RootPath(path.into()))
                .map_err(|source| Error { source })
        }

        #[derive(Debug)]
        pub struct Error {
            pub source: RootPath,
        }

        impl Display for Error {
            fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
                write!(
                    f,
                    "failed to find parent of path {}",
                    self.source.0.display()
                )
            }
        }

        impl std::error::Error for Error {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                Some(&self.source)
            }
        }

        #[derive(Debug)]
        pub struct RootPath(pub Box<fs::Path>);

        impl Display for RootPath {
            fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
                write!(f, "path {} ends in a prefix or root", self.0.display())
            }
        }

        impl std::error::Error for RootPath {}

        use crate::fs;
        use std::fmt;
        use std::fmt::Display;
        use std::fmt::Formatter;
    }

    pub use list::List;
    pub mod list {
        /// Colon-separated path list.
        #[derive(Debug)]
        #[repr(transparent)]
        pub struct List(OsStr);

        impl List {
            #[must_use]
            pub fn from_ref<S: ?Sized + AsRef<OsStr>>(s: &S) -> &Self {
                let ptr: *const OsStr = s.as_ref();
                unsafe { &*(ptr as *const Self) }
            }
            #[must_use]
            pub fn from_boxed_os_str(s: Box<OsStr>) -> Box<Self> {
                let ptr = Box::into_raw(s);
                unsafe { Box::from_raw(ptr as *mut Self) }
            }
            #[must_use]
            pub fn from_arc_os_str(s: Arc<OsStr>) -> Arc<Self> {
                let ptr = Arc::into_raw(s);
                unsafe { Arc::from_raw(ptr as *const Self) }
            }
            #[must_use]
            pub fn to_boxed(&self) -> Box<Self> {
                Self::from_boxed_os_str(self.0.into())
            }
            #[must_use]
            pub fn to_arc(&self) -> Arc<Self> {
                Self::from_arc_os_str(self.0.into())
            }
            #[must_use]
            pub fn from_env_var<Key: AsRef<OsStr>>(key: Key) -> Option<Box<Self>> {
                env::var_os(key).map(|value| Self::from_boxed_os_str(value.into_boxed_os_str()))
            }
            #[must_use]
            pub fn iter(&self) -> Iter<'_> {
                Iter(self.0.as_bytes())
            }
        }

        impl<'list> IntoIterator for &'list List {
            type Item = &'list fs::Path;
            type IntoIter = Iter<'list>;
            fn into_iter(self) -> Self::IntoIter {
                self.iter()
            }
        }

        pub struct Iter<'list>(&'list [u8]);

        impl<'list> Iterator for Iter<'list> {
            type Item = &'list fs::Path;

            fn next(&mut self) -> Option<Self::Item> {
                if self.0.is_empty() {
                    return None;
                }

                let remaining = self.0;
                let bytes = if let Some(colon) = memchr(b':', remaining) {
                    self.0 = &remaining[colon + 1..];
                    &remaining[..colon]
                } else {
                    self.0 = &[];
                    remaining
                };
                Some(fs::Path::new(OsStr::from_bytes(bytes)))
            }
        }

        impl AsRef<OsStr> for List {
            fn as_ref(&self) -> &OsStr {
                &self.0
            }
        }

        #[cfg(test)]
        mod tests {
            #[test]
            fn iteration() {
                let dirs = fs::path::List::from_ref("foo:bar:baz");
                let dirs = dirs
                    .iter()
                    .map(|path| path.as_os_str().as_bytes())
                    .collect::<Vec<_>>();
                let expected: [&[u8]; 3] = [b"foo", b"bar", b"baz"];
                assert_eq!(dirs, expected);
            }

            #[test]
            fn colon_at_end() {
                let dirs = fs::path::List::from_ref("foo:bar:baz:");
                let dirs = dirs
                    .iter()
                    .map(|path| path.as_os_str().as_bytes())
                    .collect::<Vec<_>>();
                let expected: [&[u8]; 3] = [b"foo", b"bar", b"baz"];
                assert_eq!(dirs, expected);
            }

            use crate::fs;
            use std::os::unix::ffi::OsStrExt as _;
        }

        use crate::fs;
        use memchr::memchr;
        use std::env;
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt as _;
        use std::sync::Arc;
    }
}

pub use create_dir_all::create_dir_all;
pub mod create_dir_all {
    pub fn create_dir_all(path: &fs::Path) -> Result<(), Error> {
        std::fs::create_dir_all(path).map_err(|source| Error {
            path: path.into(),
            source,
        })
    }

    #[derive(Debug)]
    pub struct Error {
        pub path: Box<fs::Path>,
        pub source: io::Error,
    }

    impl Display for Error {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            write!(f, "failed to create directory {}", self.path.display())
        }
    }

    impl std::error::Error for Error {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(&self.source)
        }
    }

    use crate::fs;
    use std::fmt;
    use std::fmt::Display;
    use std::fmt::Formatter;
    use std::io;
}

pub use rename::rename;
pub mod rename {
    pub fn rename(from: &fs::Path, to: &fs::Path) -> Result<(), Error> {
        std::fs::rename(from, to).map_err(|source| Error {
            from: from.into(),
            to: to.into(),
            source,
        })
    }

    #[derive(Debug)]
    pub struct Error {
        pub from: Box<fs::Path>,
        pub to: Box<fs::Path>,
        pub source: io::Error,
    }

    impl Display for Error {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            write!(
                f,
                "failed to rename {} to {}",
                self.from.display(),
                self.to.display()
            )
        }
    }

    impl std::error::Error for Error {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(&self.source)
        }
    }

    use crate::fs;
    use std::fmt;
    use std::fmt::Display;
    use std::fmt::Formatter;
    use std::io;
}

pub use copy::copy;
pub mod copy {
    pub fn copy(from: &fs::Path, to: &fs::Path) -> Result<u64, Error> {
        std::fs::copy(from, to).map_err(|source| Error {
            from: from.into(),
            to: to.into(),
            source,
        })
    }

    #[derive(Debug)]
    pub struct Error {
        pub from: Box<fs::Path>,
        pub to: Box<fs::Path>,
        pub source: io::Error,
    }

    impl Display for Error {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            write!(
                f,
                "failed to copy {} to {}",
                self.from.display(),
                self.to.display()
            )
        }
    }

    impl std::error::Error for Error {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(&self.source)
        }
    }

    use crate::fs;
    use std::fmt;
    use std::fmt::Display;
    use std::fmt::Formatter;
    use std::io;
}

pub use file::File;
pub mod file {
    #[derive(Debug)]
    pub struct File<P: Borrow<fs::Path>> {
        std: std::fs::File,
        path: P,
    }

    impl<P: Borrow<fs::Path>> File<P> {
        pub fn path(&self) -> &fs::Path {
            self.path.borrow()
        }
        pub fn into_path(self) -> P {
            self.path
        }
    }

    impl<P: Borrow<fs::Path>> io::Write for File<P> {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.std
                .write(buf)
                .with_context(|| format!("failed to write to {}", self.path.borrow().display()))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl<P: Borrow<fs::Path>> File<P> {
        fn read_context(&self) -> impl '_ + FnOnce() -> String {
            || format!("failed to read from {}", self.path.borrow().display())
        }
    }

    impl<P: Borrow<fs::Path>> io::Read for File<P> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.std.read(buf).with_context(self.read_context())
        }
        fn read_to_string(&mut self, buf: &mut String) -> io::Result<usize> {
            self.std
                .read_to_string(buf)
                .with_context(self.read_context())
        }
        fn read_to_end(&mut self, buf: &mut Vec<u8>) -> io::Result<usize> {
            self.std.read_to_end(buf).with_context(self.read_context())
        }
    }

    impl<P: Borrow<fs::Path>> io::Seek for File<P> {
        fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
            self.std
                .seek(pos)
                .with_context(|| format!("failed to seek in {}", self.path.borrow().display()))
        }
    }

    pub use open::open;
    pub mod open {
        pub fn open<P: Borrow<fs::Path>>(path: P, options: Options) -> Result<fs::File<P>, Error> {
            let mut std_options = std::fs::OpenOptions::new();

            std_options.read(options.access.read());

            if let Some(options) = options.access.write_options() {
                std_options.write(true);
                match options {
                    WriteOptions::OpenExisting(_) => &mut std_options,
                    WriteOptions::OpenOrCreate(_) => std_options.create(true),
                    WriteOptions::CreateNew => std_options.create_new(true),
                };
                if let WriteOptions::OpenExisting(mode) | WriteOptions::OpenOrCreate(mode) = options
                {
                    match mode {
                        WriteMode::Overwrite => &mut std_options,
                        WriteMode::Append => std_options.append(true),
                        WriteMode::Truncate => std_options.truncate(true),
                    };
                }
            }

            let std_file = std_options.open(path.borrow()).map_err(|source| Error {
                path: path.borrow().into(),
                options,
                source,
            })?;

            Ok(fs::File {
                std: std_file,
                path,
            })
        }

        pub fn read_only<P: Borrow<fs::Path>>(path: P) -> Result<fs::File<P>, Error> {
            open(path, Options::from_access(Access::ReadOnly))
        }

        #[derive(Debug, Clone)]
        #[non_exhaustive]
        pub struct Options {
            pub access: Access,
        }

        impl Options {
            #[must_use]
            pub fn from_access(access: Access) -> Self {
                Self { access }
            }
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum Access {
            ReadOnly,
            WriteOnly(WriteOptions),
            ReadWrite(WriteOptions),
        }

        impl Access {
            #[must_use]
            pub fn read(&self) -> bool {
                matches!(self, Self::ReadOnly | Self::ReadWrite(_))
            }

            #[must_use]
            pub fn write_options(&self) -> Option<WriteOptions> {
                match *self {
                    Self::ReadOnly => None,
                    Self::WriteOnly(options) | Self::ReadWrite(options) => Some(options),
                }
            }
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum WriteOptions {
            OpenExisting(WriteMode),
            OpenOrCreate(WriteMode),
            CreateNew,
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum WriteMode {
            Overwrite,
            Append,
            Truncate,
        }

        #[derive(Debug)]
        pub struct Error {
            pub path: Box<fs::Path>,
            pub options: Options,
            pub source: io::Error,
        }

        impl Display for Error {
            fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
                f.write_str("failed to ")?;
                let path = self.path.display();
                match self.options.access {
                    Access::ReadOnly => write!(f, "open {path} read-only"),
                    Access::WriteOnly(WriteOptions::OpenExisting(_)) => {
                        write!(f, "open existing file {path} write-only")
                    }
                    Access::WriteOnly(WriteOptions::OpenOrCreate(_)) => {
                        write!(f, "open {path} write-only")
                    }
                    Access::ReadWrite(WriteOptions::OpenExisting(_)) => {
                        write!(f, "open existing file {path}")
                    }
                    Access::ReadWrite(WriteOptions::OpenOrCreate(_)) => write!(f, "open {path}"),
                    Access::WriteOnly(WriteOptions::CreateNew)
                    | Access::ReadWrite(WriteOptions::CreateNew) => write!(f, "create {path}"),
                }
            }
        }

        impl std::error::Error for Error {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                Some(&self.source)
            }
        }

        use crate::fs;
        use std::borrow::Borrow;
        use std::fmt;
        use std::fmt::Display;
        use std::fmt::Formatter;
        use std::io;
    }

    use crate::fs;
    use crate::fs::io_error_context::Context as _;
    use std::borrow::Borrow;
    use std::io;
}

pub use read::read;
pub mod read {
    pub fn read<P: Borrow<fs::Path>>(path: P) -> Result<Vec<u8>, Error> {
        let path = path.borrow();
        read_inner(path).map_err(|kind| Error {
            path: path.into(),
            kind,
        })
    }

    fn read_inner(path: &fs::Path) -> Result<Vec<u8>, ErrorKind> {
        let mut file = fs::file::open::read_only(path)?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).map_err(ErrorKind::Read)?;
        Ok(bytes)
    }

    #[derive(Debug)]
    pub struct Error {
        pub path: Box<fs::Path>,
        pub kind: ErrorKind,
    }

    impl Display for Error {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            write!(f, "failed to read file {}", self.path.display())
        }
    }

    impl std::error::Error for Error {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            match &self.kind {
                ErrorKind::Open(e) => Some(e),
                ErrorKind::Read(e) => Some(e),
            }
        }
    }

    #[derive(Debug)]
    pub enum ErrorKind {
        Open(fs::file::open::Error),
        Read(io::Error),
    }

    impl From<fs::file::open::Error> for ErrorKind {
        fn from(error: fs::file::open::Error) -> Self {
            Self::Open(error)
        }
    }

    use crate::fs;
    use std::borrow::Borrow;
    use std::fmt;
    use std::fmt::Display;
    use std::fmt::Formatter;
    use std::io;
    use std::io::Read as _;
}

pub use overwrite::Overwriter;
pub mod overwrite {
    pub struct Overwriter<P: Borrow<fs::Path>> {
        final_path: P,
        temp_file: fs::File<fs::PathBuf>,
    }

    impl<P: Borrow<fs::Path>> Overwriter<P> {
        pub fn start(path: P) -> Result<Self, StartError> {
            match start_inner(path.borrow()) {
                Ok(temp_file) => Ok(Self {
                    final_path: path,
                    temp_file,
                }),
                Err(kind) => Err(StartError {
                    path: path.borrow().into(),
                    kind,
                }),
            }
        }
    }

    fn start_inner(final_path: &fs::Path) -> Result<fs::File<fs::PathBuf>, StartErrorKind> {
        let parent = fs::path::parent(final_path)?;

        fs::create_dir_all(parent)?;

        let mut temp_filename = ".DELETE_ME_".to_owned();
        rand::distributions::Alphanumeric.append_string(
            &mut rand::thread_rng(),
            &mut temp_filename,
            20,
        );
        let temp_path = parent.join(temp_filename);
        let write_options = fs::file::open::WriteOptions::CreateNew;
        let access = fs::file::open::Access::WriteOnly(write_options);
        let open_options = fs::file::open::Options::from_access(access);
        let temp_file = fs::file::open(temp_path, open_options)?;

        Ok(temp_file)
    }

    #[derive(Debug)]
    pub struct StartError {
        pub path: Box<fs::Path>,
        pub kind: StartErrorKind,
    }

    impl Display for StartError {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            write!(f, "failed to overwrite {}", self.path.display())
        }
    }

    impl std::error::Error for StartError {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            match &self.kind {
                StartErrorKind::PathParent(e) => Some(e),
                StartErrorKind::CreateDirAll(e) => Some(e),
                StartErrorKind::FileOpen(e) => Some(e),
            }
        }
    }

    #[derive(Debug)]
    pub enum StartErrorKind {
        PathParent(fs::path::parent::Error),
        CreateDirAll(fs::create_dir_all::Error),
        FileOpen(fs::file::open::Error),
    }

    impl From<fs::path::parent::Error> for StartErrorKind {
        fn from(error: fs::path::parent::Error) -> Self {
            Self::PathParent(error)
        }
    }

    impl From<fs::create_dir_all::Error> for StartErrorKind {
        fn from(error: fs::create_dir_all::Error) -> Self {
            Self::CreateDirAll(error)
        }
    }

    impl From<fs::file::open::Error> for StartErrorKind {
        fn from(error: fs::file::open::Error) -> Self {
            Self::FileOpen(error)
        }
    }

    impl<P: Borrow<fs::Path>> io::Write for Overwriter<P> {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.temp_file.write(buf).with_context(|| {
                format!("failed to overwrite {}", self.final_path.borrow().display())
            })
        }
        fn flush(&mut self) -> io::Result<()> {
            self.temp_file.flush()
        }
    }

    impl<P: Borrow<fs::Path>> Overwriter<P> {
        pub fn finish(self) -> Result<(), FinishError> {
            let temp_path = self.temp_file.into_path();

            if let Err(source) = fs::rename(&*temp_path, self.final_path.borrow()) {
                drop(std::fs::remove_file(&*temp_path));
                return Err(FinishError {
                    path: self.final_path.borrow().into(),
                    source,
                });
            }

            Ok(())
        }
    }

    #[derive(Debug)]
    pub struct FinishError {
        pub path: Box<fs::Path>,
        pub source: fs::rename::Error,
    }

    impl Display for FinishError {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            write!(f, "failed to overwrite {}", self.path.display())
        }
    }

    impl std::error::Error for FinishError {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(&self.source)
        }
    }

    pub fn with<P: Borrow<fs::Path>>(path: P, contents: &[u8]) -> Result<(), WithError> {
        (|| {
            let mut overwriter = Overwriter::start(path)?;
            overwriter
                .write_all(contents)
                .map_err(WithErrorKind::Write)?;
            overwriter.finish()?;
            Ok(())
        })()
        .map_err(|kind| WithError { kind })
    }

    #[derive(Debug)]
    pub struct WithError {
        kind: WithErrorKind,
    }

    impl WithError {
        fn inner(&self) -> &(dyn 'static + std::error::Error) {
            match &self.kind {
                WithErrorKind::Start(e) => e,
                WithErrorKind::Write(e) => e,
                WithErrorKind::Finish(e) => e,
            }
        }
    }

    impl Display for WithError {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            Display::fmt(self.inner(), f)
        }
    }

    impl std::error::Error for WithError {
        fn source(&self) -> Option<&(dyn 'static + std::error::Error)> {
            self.inner().source()
        }
    }

    #[derive(Debug)]
    enum WithErrorKind {
        Start(StartError),
        Write(io::Error),
        Finish(FinishError),
    }

    impl From<StartError> for WithErrorKind {
        fn from(error: StartError) -> Self {
            WithErrorKind::Start(error)
        }
    }

    impl From<FinishError> for WithErrorKind {
        fn from(error: FinishError) -> Self {
            WithErrorKind::Finish(error)
        }
    }

    use crate::fs;
    use crate::fs::io_error_context::Context as _;
    use rand::distributions::DistString;
    use std::borrow::Borrow;
    use std::fmt;
    use std::fmt::Display;
    use std::fmt::Formatter;
    use std::io;
    use std::io::Write as _;
}

pub use read_dir::read_dir;
pub mod read_dir {
    pub fn read_dir<P: Borrow<fs::Path>>(path: P) -> Result<ReadDir<P>, Error> {
        let std = std::fs::read_dir(path.borrow()).map_err(|source| Error {
            path: path.borrow().into(),
            source,
        })?;
        Ok(ReadDir { std, path })
    }

    pub struct ReadDir<P: Borrow<fs::Path>> {
        std: std::fs::ReadDir,
        path: P,
    }

    impl<P: Borrow<fs::Path>> Iterator for ReadDir<P> {
        type Item = Result<std::fs::DirEntry, Error>;

        fn next(&mut self) -> Option<Self::Item> {
            Some(self.std.next()?.map_err(|source| Error {
                path: self.path.borrow().into(),
                source,
            }))
        }
    }

    #[derive(Debug)]
    pub struct Error {
        pub path: Box<fs::Path>,
        pub source: io::Error,
    }

    impl Display for Error {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            write!(f, "failed to read directory {}", self.path.display())
        }
    }

    impl std::error::Error for Error {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(&self.source)
        }
    }

    use crate::fs;
    use std::borrow::Borrow;
    use std::fmt;
    use std::fmt::Display;
    use std::fmt::Formatter;
    use std::io;
}

mod io_error_context {
    /// This is a miserable little function. If only the I/O traits didn't enforce using
    /// `io::Error` as an error type.
    pub(crate) fn io_error_context(error: io::Error, context: String) -> io::Error {
        io::Error::new(
            error.kind(),
            Wrapper {
                context,
                source: error,
            },
        )
    }

    #[derive(Debug)]
    struct Wrapper {
        context: String,
        source: io::Error,
    }

    impl Display for Wrapper {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            f.write_str(&*self.context)
        }
    }

    impl std::error::Error for Wrapper {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(&self.source)
        }
    }

    pub(crate) trait Context {
        type Output;
        fn with_context<F: FnOnce() -> String>(self, f: F) -> io::Result<Self::Output>;
    }

    impl<T> Context for io::Result<T> {
        type Output = T;
        fn with_context<F: FnOnce() -> String>(self, f: F) -> io::Result<Self::Output> {
            self.map_err(|error| io_error_context(error, f()))
        }
    }

    use std::fmt;
    use std::fmt::Display;
    use std::fmt::Formatter;
    use std::io;
}
