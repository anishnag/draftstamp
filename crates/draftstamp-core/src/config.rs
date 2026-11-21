//! `.draftstamp`: per-folder settings, never stamped.
//!
//! `key = "value"` lines; blank lines and `#` comments are ignored, and unknown keys are ignored
//! so settings can be added later. Today the only key is `project`.

use std::fs;
use std::path::Path;

use crate::Error;

pub(crate) const CONFIG_FILE: &str = ".draftstamp";

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Config {
    pub(crate) project: Option<String>,
}

impl Config {
    /// Reads `<folder>/.draftstamp`; an absent file means default settings.
    pub(crate) fn load(folder: &Path) -> Result<Config, Error> {
        match fs::read_to_string(folder.join(CONFIG_FILE)) {
            Ok(text) => Ok(Config::parse(&text)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(e.into()),
        }
    }

    pub(crate) fn parse(text: &str) -> Config {
        let mut config = Config::default();
        for line in text.lines().map(str::trim) {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            let value = value
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .unwrap_or(value);
            if key.trim() == "project" && !value.is_empty() {
                config.project = Some(value.to_owned());
            }
        }
        config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses() {
        let c = Config::parse("# settings\nproject = \"riemann-draft\"\nfuture = 1\n");
        assert_eq!(c.project.as_deref(), Some("riemann-draft"));
        assert_eq!(
            Config::parse("project = bare words").project.as_deref(),
            Some("bare words")
        );
        assert_eq!(Config::parse("project = \"\"").project, None);
        assert_eq!(Config::parse("").project, None);
    }
}
