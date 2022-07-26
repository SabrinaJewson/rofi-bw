pub(crate) fn load(path: &fs::Path) -> anyhow::Result<Config> {
    load_inner(path).context("failed to load config file")
}

fn load_inner(path: &fs::Path) -> anyhow::Result<Config> {
    let bytes = match fs::read(&*path) {
        Ok(bytes) => bytes,
        Err(fs::read::Error {
            kind: fs::read::ErrorKind::Open(e),
            ..
        }) if e.source.kind() == io::ErrorKind::NotFound => return Ok(Config::default()),
        Err(e) => return Err(e.into()),
    };

    let config = toml::from_slice::<Config>(&*bytes)
        .with_context(|| format!("{} is invalid", path.display()))?;

    Ok(config)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Config {
    #[serde(default)]
    pub(crate) auto_lock: AutoLock,

    #[serde(default = "returns_true")]
    pub(crate) copy_notification: bool,

    #[serde(default)]
    pub(crate) rofi_options: RofiOptions,

    #[serde(default = "desktop_string")]
    pub(crate) client_id: String,

    #[serde(default = "linux_string")]
    pub(crate) device_name: String,

    #[serde(default = "linux_desktop_device_type", with = "device_type")]
    pub(crate) device_type: auth::DeviceType,
}

impl Default for Config {
    fn default() -> Self {
        serde_default()
    }
}

mod device_type {
    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<auth::DeviceType, D::Error> {
        deserializer.deserialize_str(Visitor)
    }

    struct Visitor;
    impl<'de> de::Visitor<'de> for Visitor {
        type Value = auth::DeviceType;
        fn expecting(&self, f: &mut Formatter<'_>) -> fmt::Result {
            f.write_str("a device type")
        }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
            let (device_type, _) = auth::DeviceType::DISPLAY_NAMES
                .into_iter()
                .find(|(_, display)| display.eq_ignore_ascii_case(v))
                .ok_or_else(|| de::Error::custom(DeError { v }))?;
            Ok(device_type)
        }
    }

    struct DeError<'v> {
        v: &'v str,
    }
    impl Display for DeError<'_> {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            write!(f, "unknown device type `{}`, expected one of ", self.v)?;
            for (i, (_, display)) in auth::DeviceType::DISPLAY_NAMES.into_iter().enumerate() {
                if i > 0 {
                    f.write_str(", ")?;
                }
                f.write_str(display)?;
            }
            Ok(())
        }
    }

    use crate::auth;
    use core::fmt;
    use serde::de;
    use serde::Deserializer;
    use std::fmt::Display;
    use std::fmt::Formatter;
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RofiOptions {
    #[serde(default = "usr_bin_rofi_string")]
    pub(crate) binary: String,

    #[serde(default)]
    pub(crate) threads: u32,

    #[serde(default)]
    pub(crate) case_sensitive: bool,

    #[serde(default)]
    pub(crate) cycle: Option<bool>,

    #[serde(default)]
    pub(crate) config: Option<fs::PathBuf>,

    #[serde(default)]
    pub(crate) scroll_method: Option<ScrollMethod>,

    #[serde(default)]
    pub(crate) normalize_match: bool,

    #[serde(default = "returns_true")]
    pub(crate) lazy_grab: bool,

    #[serde(default)]
    pub(crate) normal_window: bool,

    #[serde(default)]
    pub(crate) matching: Option<Matching>,

    #[serde(default)]
    pub(crate) matching_negate_char: Option<String>,

    #[serde(default)]
    pub(crate) theme: Option<String>,

    #[serde(default)]
    pub(crate) theme_str: String,

    #[serde(default = "returns_true")]
    pub(crate) click_to_exit: bool,
}

impl Default for RofiOptions {
    fn default() -> Self {
        serde_default()
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ScrollMethod {
    PerPage,
    Continuous,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Matching {
    Normal,
    Regex,
    Glob,
    Fuzzy,
    Prefix,
}

pub(crate) use auto_lock::AutoLock;
mod auto_lock {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) enum AutoLock {
        Never,
        After(Duration),
    }

    impl Default for AutoLock {
        fn default() -> Self {
            Self::After(Duration::from_secs(15))
        }
    }

    impl<'de> Deserialize<'de> for AutoLock {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            deserializer.deserialize_str(Visitor)
        }
    }

    struct Visitor;
    impl<'de> de::Visitor<'de> for Visitor {
        type Value = AutoLock;
        fn expecting(&self, f: &mut Formatter<'_>) -> fmt::Result {
            f.write_str("a duration or `never`")
        }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
            parse(v).ok_or_else(|| de::Error::invalid_value(de::Unexpected::Str(v), &self))
        }
    }

    fn parse(s: &str) -> Option<AutoLock> {
        if s.eq_ignore_ascii_case("never") {
            return Some(AutoLock::Never);
        }

        let s = s.trim();

        let multiplier = match s.chars().last()? {
            'h' => 60 * 60,
            'm' => 60,
            's' => 1,
            _ => return None,
        };

        let number = s[..s.len() - 1].parse::<u64>().ok()?;

        let seconds = number.checked_mul(multiplier)?;

        Some(AutoLock::After(Duration::from_secs(seconds)))
    }

    #[test]
    fn test_parse() {
        assert_eq!(parse(""), None);
        assert_eq!(parse("  "), None);
        assert_eq!(parse("NeVEr"), Some(AutoLock::Never));
        assert_eq!(parse("0s"), Some(AutoLock::After(Duration::from_secs(0))));
        assert_eq!(
            parse("5938s"),
            Some(AutoLock::After(Duration::from_secs(5938)))
        );
        assert_eq!(
            parse("12m"),
            Some(AutoLock::After(Duration::from_secs(12 * 60)))
        );
        assert_eq!(
            parse("\t1h\t"),
            Some(AutoLock::After(Duration::from_secs(60 * 60)))
        );
    }

    use serde::de;
    use serde::Deserialize;
    use serde::Deserializer;
    use std::fmt;
    use std::fmt::Formatter;
    use std::time::Duration;
}

fn desktop_string() -> String {
    "desktop".to_owned()
}

fn linux_string() -> String {
    "linux".to_owned()
}

fn linux_desktop_device_type() -> auth::DeviceType {
    auth::DeviceType::LinuxDesktop
}

fn usr_bin_rofi_string() -> String {
    "/usr/bin/rofi".to_owned()
}

fn returns_true() -> bool {
    true
}

use serde_default::serde_default;
mod serde_default {
    pub(crate) fn serde_default<'de, T: Deserialize<'de>>() -> T {
        T::deserialize(EmptyMap).unwrap()
    }

    struct EmptyMap;

    impl<'de> Deserializer<'de> for EmptyMap {
        type Error = Error;

        fn deserialize_any<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
            visitor.visit_map(self)
        }

        serde::forward_to_deserialize_any! {
            bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
            bytes byte_buf option unit unit_struct newtype_struct seq tuple
            tuple_struct map struct enum identifier ignored_any
        }
    }

    impl<'de> de::MapAccess<'de> for EmptyMap {
        type Error = Error;

        fn next_key_seed<K>(&mut self, _seed: K) -> Result<Option<K::Value>, Self::Error>
        where
            K: DeserializeSeed<'de>,
        {
            Ok(None)
        }

        fn next_value_seed<V>(&mut self, _seed: V) -> Result<V::Value, Self::Error>
        where
            V: DeserializeSeed<'de>,
        {
            panic!("map is empty")
        }
    }

    #[derive(Debug)]
    struct Error;

    impl Display for Error {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            f.write_str("failed to construct Serde default of type")
        }
    }

    impl std::error::Error for Error {}

    impl de::Error for Error {
        fn custom<T: Display>(_: T) -> Self {
            Self
        }
    }

    use serde::de;
    use serde::de::Deserialize;
    use serde::de::DeserializeSeed;
    use serde::Deserializer;
    use std::fmt;
    use std::fmt::Display;
    use std::fmt::Formatter;
}

use crate::auth;
use anyhow::Context as _;
use rofi_bw_common::fs;
use serde::Deserialize;
use std::io;
