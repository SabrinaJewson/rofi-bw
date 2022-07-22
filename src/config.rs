#[derive(Debug, Default, Deserialize)]
pub(crate) struct Config {
    #[serde(default)]
    pub(crate) auto_lock: AutoLock,

    #[serde(default = "default_copy_notification")]
    pub(crate) copy_notification: bool,
}

fn default_copy_notification() -> bool {
    true
}

pub(crate) fn load(config_dir: &Path) -> anyhow::Result<Config> {
    load_inner(config_dir).context("failed to load config file")
}

fn load_inner(config_dir: &Path) -> anyhow::Result<Config> {
    let path = config_dir.join("config.toml");

    let bytes = match fs::read(&*path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Config::default()),
        Err(e) => return Err(e).context(format!("failed to read {}", path.display())),
    };

    let config = toml::from_slice::<Config>(&*bytes)
        .with_context(|| format!("{} is invalid", path.display()))?;

    Ok(config)
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

use anyhow::Context as _;
use serde::Deserialize;
use std::fs;
use std::io;
use std::path::Path;
