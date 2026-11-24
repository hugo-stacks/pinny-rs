use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};

include!("include/macros.rs");

#[derive(Debug)]
pub struct Config {
    pub allowed_tags: Vec<String>,
}

impl Config {
    /// Get the config for the crate currently being expanded, cached per
    /// manifest directory. A single proc-macro server process (e.g.
    /// rust-analyzer's) expands macros for every crate in a workspace, so a
    /// process-wide singleton would leak the first crate's allowed tags into
    /// all the others.
    pub(crate) fn get_instance() -> Result<Arc<Self>, ConfigError> {
        Self::get_instance_for(&ConfigFactory::manifest_dir()?)
    }

    /// Get the config for the crate rooted at `manifest_dir`, reading its
    /// `Cargo.toml` only the first time that directory is requested.
    pub(crate) fn get_instance_for(
        manifest_dir: &str,
    ) -> Result<Arc<Self>, ConfigError> {
        static CACHE: OnceLock<Mutex<HashMap<String, Arc<Config>>>> =
            OnceLock::new();
        let mut cache = CACHE
            .get_or_init(Default::default)
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(config) = cache.get(manifest_dir) {
            debug!("Config cache hit for {manifest_dir}: {config:?}");
            return Ok(Arc::clone(config));
        }
        let config = Arc::new(ConfigFactory::create_for(manifest_dir)?);
        debug!("Config initialized for {manifest_dir}: {config:?}");
        cache.insert(manifest_dir.to_owned(), Arc::clone(&config));
        drop(cache);
        Ok(config)
    }
}

#[derive(Debug)]
pub enum ConfigError {
    MissingEnvVar(std::env::VarError),
    ReadError(std::io::Error),
    ParseError(toml::de::Error),
    MissingTags,
    InvalidArrayFormat,
    InvalidStringType,
    DuplicateTag(String),
    InvalidTagFormat(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingEnvVar(error) => {
                write!(f, "Failed to get CARGO_MANIFEST_DIR: {error}")
            }
            Self::ReadError(error) => {
                write!(f, "Failed to read Cargo.toml: {error}")
            }
            Self::ParseError(error) => {
                write!(f, "Invalid TOML format: {error}")
            }
            Self::MissingTags => {
                f.write_str("Missing `allowed` tags in Cargo.toml")
            }
            Self::InvalidArrayFormat => {
                f.write_str("`allowed` tags must be an array")
            }
            Self::InvalidStringType => {
                f.write_str("Each tag should be a string")
            }
            Self::DuplicateTag(tag) => {
                write!(f, "Duplicated tag found: {tag}")
            }
            Self::InvalidTagFormat(tag) => {
                write!(f, "Tag is not a valid rust identifier: {tag}")
            }
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::MissingEnvVar(error) => Some(error),
            Self::ReadError(error) => Some(error),
            Self::ParseError(error) => Some(error),
            Self::MissingTags
            | Self::InvalidArrayFormat
            | Self::InvalidStringType
            | Self::DuplicateTag(_)
            | Self::InvalidTagFormat(_) => None,
        }
    }
}

impl From<std::env::VarError> for ConfigError {
    fn from(error: std::env::VarError) -> Self {
        Self::MissingEnvVar(error)
    }
}

impl From<std::io::Error> for ConfigError {
    fn from(error: std::io::Error) -> Self {
        Self::ReadError(error)
    }
}

impl From<toml::de::Error> for ConfigError {
    fn from(error: toml::de::Error) -> Self {
        Self::ParseError(error)
    }
}

impl PartialEq for ConfigError {
    fn eq(&self, other: &Self) -> bool {
        use ConfigError::{
            DuplicateTag, InvalidArrayFormat, InvalidStringType,
            InvalidTagFormat, MissingEnvVar, MissingTags, ParseError,
            ReadError,
        };
        match (self, other) {
            (MissingEnvVar(_), MissingEnvVar(_))
            | (ReadError(_), ReadError(_))
            | (ParseError(_), ParseError(_))
            | (MissingTags, MissingTags)
            | (InvalidArrayFormat, InvalidArrayFormat)
            | (InvalidStringType, InvalidStringType) => true,
            (DuplicateTag(s1), DuplicateTag(s2))
            | (InvalidTagFormat(s1), InvalidTagFormat(s2)) => s1 == s2,
            _ => false,
        }
    }
}

pub struct ConfigFactory {
    _prevent_instance: (),
}

impl ConfigFactory {
    /// Resolve the manifest directory of the crate being expanded.
    pub fn manifest_dir() -> Result<String, ConfigError> {
        // Introduced `PINNY_CARGO_MANIFEST_DIR` as on override for `CARGO_MANIFEST_DIR` to execute tests with trybuilder crate
        // This workaroubnd is suggested even here https://github.com/dtolnay/trybuild/issues/202
        Ok(std::env::var("PINNY_CARGO_MANIFEST_DIR")
            .or_else(|_| std::env::var("CARGO_MANIFEST_DIR"))?)
    }

    pub fn create_for(manifest_dir: &str) -> Result<Config, ConfigError> {
        let cargo_path = std::path::Path::new(manifest_dir).join("Cargo.toml");
        let cargo_string = std::fs::read_to_string(cargo_path)?;
        let cargo_toml = toml::de::DeTable::parse(&cargo_string)?;

        let allowed_tags = cargo_toml
            .get_ref()
            .get("package")
            .and_then(|pkg| pkg.get_ref().get("metadata"))
            .and_then(|meta| meta.get_ref().get("pinny"))
            .and_then(|tags| tags.get_ref().get("allowed"))
            .ok_or(ConfigError::MissingTags)?
            .get_ref()
            .as_array()
            .ok_or(ConfigError::InvalidArrayFormat)?
            .iter()
            .map(|mode| {
                mode.get_ref()
                    .as_str()
                    .map(std::string::ToString::to_string)
                    .ok_or(ConfigError::InvalidStringType)
            })
            .collect::<Result<Vec<String>, ConfigError>>()?;

        //check for duplication and format (as rust identifier)
        let mut seen = HashSet::new();
        for tag in &allowed_tags {
            if !seen.insert(tag) {
                return Err(ConfigError::DuplicateTag(tag.clone()));
            }
            if !Self::is_valid_tag(tag) {
                return Err(ConfigError::InvalidTagFormat(tag.clone()));
            }
        }
        Ok(Config { allowed_tags })
    }

    /// A valid tag is an ASCII lowercase letter followed by ASCII lowercase
    /// letters, digits or `_`.
    fn is_valid_tag(tag: &str) -> bool {
        let mut chars = tag.chars();
        chars.next().is_some_and(|first| first.is_ascii_lowercase())
            && chars.all(|c| {
                c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'
            })
    }
}
