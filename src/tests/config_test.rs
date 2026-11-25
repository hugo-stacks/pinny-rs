use crate::config::*;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// A directory under the system temp dir that is removed on drop.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        let name = format!(
            "pinny-test-{}-{}-{nanos}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let path = std::env::temp_dir().join(name);
        std::fs::create_dir(&path).expect("failed to create temp dir");
        Self { path }
    }

    fn manifest_dir(&self) -> &str {
        self.path.to_str().expect("temp dir path is not UTF-8")
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn create_cargo_toml(content: &str) -> TempDir {
    let tmp_dir = TempDir::new();
    std::fs::write(tmp_dir.path.join("Cargo.toml"), content)
        .expect("failed to write Cargo.toml file");
    tmp_dir
}

#[test]
fn test_create_config_with_3_tags_ok() {
    let content = r#"
[Package]
name = "consumer"
version = "0.0.1"

[package.metadata.pinny]
allowed = ["tag1", "tag2", "tag3"]
"#;

    let tmp_dir = create_cargo_toml(content);

    let config_result = ConfigFactory::create_for(tmp_dir.manifest_dir());
    assert!(config_result.is_ok());

    let config = config_result.unwrap();

    assert_eq!(3, config.allowed_tags.len());
    assert_eq!("tag1", config.allowed_tags[0]);
    assert_eq!("tag2", config.allowed_tags[1]);
    assert_eq!("tag3", config.allowed_tags[2]);
}

#[test]
fn test_get_instance_caches_config_per_manifest_dir_ok() {
    let content_a = r#"
[Package]
name = "consumer_a"
version = "0.0.1"

[package.metadata.pinny]
allowed = ["tag_a"]
"#;
    let content_b = r#"
[Package]
name = "consumer_b"
version = "0.0.1"

[package.metadata.pinny]
allowed = ["tag_b"]
"#;

    let tmp_dir_a = create_cargo_toml(content_a);
    let tmp_dir_b = create_cargo_toml(content_b);

    // Simulate a single long-lived process (e.g. rust-analyzer's proc-macro
    // server) expanding macros for two crates with different allowed tags.
    let config_a = Config::get_instance_for(tmp_dir_a.manifest_dir()).unwrap();
    assert_eq!(vec!["tag_a".to_string()], config_a.allowed_tags);

    let config_b = Config::get_instance_for(tmp_dir_b.manifest_dir()).unwrap();
    assert_eq!(vec!["tag_b".to_string()], config_b.allowed_tags);

    // The first crate's config must still be served on a cache hit.
    let config_a_again =
        Config::get_instance_for(tmp_dir_a.manifest_dir()).unwrap();
    assert_eq!(vec!["tag_a".to_string()], config_a_again.allowed_tags);
    assert!(Arc::ptr_eq(&config_a, &config_a_again));
}

#[test]
fn test_create_config_with_missing_file_ko() {
    let tmp_dir = TempDir::new();

    let config_result = ConfigFactory::create_for(tmp_dir.manifest_dir());
    assert!(config_result.is_err());
    assert!(matches!(
        config_result.err().unwrap(),
        ConfigError::ReadError(_)
    ));
}

#[test]
fn test_create_config_with_invalid_toml_ko() {
    let content = r#"
[Pa
name = "consumer"
version = "0.0.1"

[package.metadata.pinny]
"#;

    let tmp_dir = create_cargo_toml(content);

    let config_result = ConfigFactory::create_for(tmp_dir.manifest_dir());
    assert!(config_result.is_err());
    assert!(matches!(
        config_result.err().unwrap(),
        ConfigError::ParseError(_)
    ));
}

#[test]
fn test_create_config_with_missing_allowed_tags_ko() {
    let content = r#"
[Package]
name = "consumer"
version = "0.0.1"

[package.metadata.pinny]
"#;

    let tmp_dir = create_cargo_toml(content);

    let config_result = ConfigFactory::create_for(tmp_dir.manifest_dir());
    assert!(config_result.is_err());
    assert_eq!(ConfigError::MissingTags, config_result.err().unwrap());
}

#[test]
fn test_create_config_with_missing_metadata_ko() {
    let content = r#"
[Package]
name = "consumer"
version = "0.0.1"
"#;

    let tmp_dir = create_cargo_toml(content);

    let config_result = ConfigFactory::create_for(tmp_dir.manifest_dir());
    assert!(config_result.is_err());
    assert_eq!(ConfigError::MissingTags, config_result.err().unwrap());
}

#[test]
fn test_create_config_with_duplicated_tag_ko() {
    let content = r#"
[Package]
name = "consumer"
version = "0.0.1"

[package.metadata.pinny]
allowed = ["tag1", "tag1", "tag3"]
"#;

    let tmp_dir = create_cargo_toml(content);

    let config_result = ConfigFactory::create_for(tmp_dir.manifest_dir());
    assert!(config_result.is_err());
    assert_eq!(
        ConfigError::DuplicateTag("tag1".into()),
        config_result.err().unwrap()
    );
}

#[test]
fn test_create_config_with_invalid_tag_format_ko() {
    let content = r#"
[Package]
name = "consumer"
version = "0.0.1"

[package.metadata.pinny]
allowed = ["?invalid", "tag2", "tag3"]
"#;

    let tmp_dir = create_cargo_toml(content);

    let config_result = ConfigFactory::create_for(tmp_dir.manifest_dir());
    assert!(config_result.is_err());
    assert_eq!(
        ConfigError::InvalidTagFormat("?invalid".into()),
        config_result.err().unwrap()
    );
}

#[test]
fn test_create_config_with_invalid_tags_array_ko() {
    let content = r#"
[Package]
name = "consumer"
version = "0.0.1"

[package.metadata.pinny]
allowed = "tag1"
"#;

    let tmp_dir = create_cargo_toml(content);

    let config_result = ConfigFactory::create_for(tmp_dir.manifest_dir());
    assert!(config_result.is_err());
    assert_eq!(
        ConfigError::InvalidArrayFormat,
        config_result.err().unwrap()
    );
}

#[test]
fn test_is_valid_tag() {
    for valid in ["a", "tag", "tag_1", "t__", "a0_b9_z"] {
        assert!(ConfigFactory::is_valid_tag(valid), "{valid:?}");
    }

    for invalid in [
        "", "_tag", "1tag", "Tag", "tAg", "tag-1", "tag 1", " tag", "tag\n",
        "tàg", "étag", "tag１",
    ] {
        assert!(!ConfigFactory::is_valid_tag(invalid), "{invalid:?}");
    }
}
