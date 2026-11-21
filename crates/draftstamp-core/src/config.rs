//! `.draftstamp`: per-folder settings, never stamped.
//!
//! `key = "value"` lines; blank lines and `#` comments are ignored, and unknown keys are ignored
//! so settings can be added later. Today the only key is `project`.

use std::fs;
use std::path::Path;

use crate::Error;

/// The settings file's name.
pub const SETTINGS_FILE: &str = ".draftstamp";

/// A folder's `.draftstamp` settings.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    /// The project tag for stamps of this folder.
    pub project: Option<String>,
}

impl Settings {
    /// Reads `<folder>/.draftstamp`; an absent file means default settings.
    pub fn load(folder: &Path) -> Result<Settings, Error> {
        match fs::read_to_string(folder.join(SETTINGS_FILE)) {
            Ok(text) => Ok(Settings::parse(&text)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Settings::default()),
            Err(e) => Err(e.into()),
        }
    }

    /// Parses the settings text.
    pub fn parse(text: &str) -> Settings {
        let mut settings = Settings::default();
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
                settings.project = Some(value.to_owned());
            }
        }
        settings
    }

    /// The settings as file text. Values must be single lines.
    pub fn to_text(&self) -> String {
        let mut text = String::from("# DraftStamp settings for this folder. Never stamped.\n");
        if let Some(project) = &self.project {
            text.push_str(&format!("project = \"{project}\"\n"));
        }
        text
    }
}

/// The project a stamp of `path` (a folder or a file) is tagged with by default: `project` in
/// the folder's `.draftstamp` (for a file, its folder's), else the folder's name (for a file,
/// the file's name).
pub fn default_project(path: &Path) -> Result<Option<String>, Error> {
    let path = path.canonicalize()?;
    let folder = if path.is_dir() {
        path.as_path()
    } else {
        path.parent().unwrap_or(&path)
    };
    Ok(Settings::load(folder)?
        .project
        .or_else(|| path.file_name()?.to_str().map(str::to_owned)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses() {
        let c = Settings::parse("# settings\nproject = \"riemann-draft\"\nfuture = 1\n");
        assert_eq!(c.project.as_deref(), Some("riemann-draft"));
        assert_eq!(
            Settings::parse("project = bare words").project.as_deref(),
            Some("bare words")
        );
        assert_eq!(Settings::parse("project = \"\"").project, None);
        assert_eq!(Settings::parse("").project, None);
    }

    #[test]
    fn round_trips() {
        let s = Settings {
            project: Some("riemann draft".into()),
        };
        assert_eq!(Settings::parse(&s.to_text()), s);
    }
}
