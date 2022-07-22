pub(crate) fn load(config_dir: &fs::Path) -> anyhow::Result<Config> {
    load_inner(config_dir).context("failed to load config file")
}

fn load_inner(config_dir: &fs::Path) -> anyhow::Result<Config> {
    let path = config_dir.join("config.toml");

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

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Config {
    #[serde(default)]
    pub(crate) auto_lock: AutoLock,

    #[serde(default = "returns_true")]
    pub(crate) copy_notification: bool,

    #[serde(default)]
    pub(crate) rofi_options: RofiOptions,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RofiOptions {
    #[serde(default = "default_binary")]
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

fn default_binary() -> String {
    "/usr/bin/rofi".to_owned()
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

fn returns_true() -> bool {
    true
}

use anyhow::Context as _;
use rofi_bw_common::fs;
use serde::Deserialize;
use std::io;
