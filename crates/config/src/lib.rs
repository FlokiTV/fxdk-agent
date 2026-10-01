use std::{
    error::Error,
    fmt, fs, io,
    path::{Path, PathBuf},
};

use directories::BaseDirs;
use serde::{Deserialize, Serialize};

pub const CONFIG_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub schema_version: u32,
    pub server_project: Option<PathBuf>,
    pub fxserver_path: Option<PathBuf>,
    pub fivem_path: Option<PathBuf>,
    pub synthetic_identity: SyntheticIdentityConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            schema_version: CONFIG_SCHEMA_VERSION,
            server_project: None,
            fxserver_path: None,
            fivem_path: None,
            synthetic_identity: SyntheticIdentityConfig::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SyntheticIdentityConfig {
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ConfigPatch {
    pub server_project: Option<Option<PathBuf>>,
    pub fxserver_path: Option<Option<PathBuf>>,
    pub fivem_path: Option<Option<PathBuf>>,
    pub synthetic_identity: Option<SyntheticIdentityPatch>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SyntheticIdentityPatch {
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigValidationIssue {
    pub field: String,
    pub code: String,
    pub message: String,
}

pub fn validate_runtime_paths(config: &AppConfig) -> Vec<ConfigValidationIssue> {
    let mut issues = Vec::new();

    if let Some(path) = &config.server_project {
        if !path.exists() {
            issues.push(validation_issue(
                "serverProject",
                "PATH_NOT_FOUND",
                format!("server project does not exist: {}", path.display()),
            ));
        } else if !path.is_dir() {
            issues.push(validation_issue(
                "serverProject",
                "NOT_A_DIRECTORY",
                format!("server project is not a directory: {}", path.display()),
            ));
        }
    }

    validate_executable_path("fxserverPath", config.fxserver_path.as_deref(), &mut issues);
    validate_executable_path("fivemPath", config.fivem_path.as_deref(), &mut issues);

    issues
}

fn validate_executable_path(
    field: &str,
    path: Option<&Path>,
    issues: &mut Vec<ConfigValidationIssue>,
) {
    let Some(path) = path else {
        return;
    };

    if !path.exists() {
        issues.push(validation_issue(
            field,
            "PATH_NOT_FOUND",
            format!("executable does not exist: {}", path.display()),
        ));
    } else if !path.is_file() {
        issues.push(validation_issue(
            field,
            "NOT_A_FILE",
            format!("executable path is not a file: {}", path.display()),
        ));
    }
}

fn validation_issue(
    field: &str,
    code: &str,
    message: String,
) -> ConfigValidationIssue {
    ConfigValidationIssue {
        field: field.to_owned(),
        code: code.to_owned(),
        message,
    }
}

#[derive(Debug)]
pub enum ConfigStoreError {
    LocalDataDirectoryUnavailable,
    Io(io::Error),
    Json(serde_json::Error),
    UnsupportedSchema { found: u32, supported: u32 },
}

impl fmt::Display for ConfigStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LocalDataDirectoryUnavailable => {
                write!(formatter, "local data directory is unavailable")
            }
            Self::Io(error) => write!(formatter, "config I/O failed: {error}"),
            Self::Json(error) => write!(formatter, "config JSON is invalid: {error}"),
            Self::UnsupportedSchema { found, supported } => write!(
                formatter,
                "unsupported config schema version {found}; supported version is {supported}"
            ),
        }
    }
}

impl Error for ConfigStoreError {}

impl From<io::Error> for ConfigStoreError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for ConfigStoreError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

#[derive(Debug, Clone)]
pub struct ConfigStore {
    path: PathBuf,
}

impl ConfigStore {
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn default_local() -> Result<Self, ConfigStoreError> {
        Ok(Self::at(default_config_path()?))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Result<AppConfig, ConfigStoreError> {
        if !self.path.exists() {
            return Ok(AppConfig::default());
        }

        let bytes = fs::read(&self.path)?;
        let config: AppConfig = serde_json::from_slice(&bytes)?;
        validate_schema(&config)?;

        Ok(config)
    }

    pub fn load_or_create(&self) -> Result<AppConfig, ConfigStoreError> {
        if self.path.exists() {
            return self.load();
        }

        let config = AppConfig::default();
        self.save(&config)?;
        Ok(config)
    }

    pub fn save(&self, config: &AppConfig) -> Result<(), ConfigStoreError> {
        validate_schema(config)?;

        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut bytes = serde_json::to_vec_pretty(config)?;
        bytes.push(b'\n');
        fs::write(&self.path, bytes)?;

        Ok(())
    }
}

pub fn default_config_path() -> Result<PathBuf, ConfigStoreError> {
    let base_dirs = BaseDirs::new().ok_or(ConfigStoreError::LocalDataDirectoryUnavailable)?;

    Ok(base_dirs
        .data_local_dir()
        .join("FXDK Agent")
        .join("config.json"))
}

fn validate_schema(config: &AppConfig) -> Result<(), ConfigStoreError> {
    if config.schema_version != CONFIG_SCHEMA_VERSION {
        return Err(ConfigStoreError::UnsupportedSchema {
            found: config.schema_version,
            supported: CONFIG_SCHEMA_VERSION,
        });
    }

    Ok(())
}

impl AppConfig {
    pub fn apply_patch(&mut self, patch: ConfigPatch) {
        if let Some(server_project) = patch.server_project {
            self.server_project = server_project;
        }
        if let Some(fxserver_path) = patch.fxserver_path {
            self.fxserver_path = fxserver_path;
        }
        if let Some(fivem_path) = patch.fivem_path {
            self.fivem_path = fivem_path;
        }
        if let Some(synthetic_identity) = patch.synthetic_identity
            && let Some(enabled) = synthetic_identity.enabled
        {
            self.synthetic_identity.enabled = enabled;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf, process};

    use super::{
        AppConfig, CONFIG_SCHEMA_VERSION, ConfigPatch, ConfigStore, ConfigStoreError,
        SyntheticIdentityConfig, SyntheticIdentityPatch, validate_runtime_paths,
    };

    fn test_store(name: &str) -> ConfigStore {
        let root = std::env::temp_dir().join(format!(
            "fxdk-agent-config-test-{}-{name}",
            process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        ConfigStore::at(root.join("config.json"))
    }

    #[test]
    fn default_config_is_schema_v1_and_dev_identity_is_disabled() {
        let config = AppConfig::default();

        assert_eq!(config.schema_version, CONFIG_SCHEMA_VERSION);
        assert_eq!(config.server_project, None);
        assert_eq!(config.fxserver_path, None);
        assert_eq!(config.fivem_path, None);
        assert!(!config.synthetic_identity.enabled);
    }

    #[test]
    fn config_serializes_with_public_camel_case_contract() {
        let config = AppConfig {
            schema_version: CONFIG_SCHEMA_VERSION,
            server_project: Some(PathBuf::from(r"D:\server")),
            fxserver_path: Some(PathBuf::from(r"D:\cfx\FXServer.exe")),
            fivem_path: Some(PathBuf::from(r"C:\FiveM\FiveM.exe")),
            synthetic_identity: SyntheticIdentityConfig { enabled: true },
        };

        let value = serde_json::to_value(config).expect("serialize config");

        assert_eq!(value["schemaVersion"], CONFIG_SCHEMA_VERSION);
        assert_eq!(value["serverProject"], r"D:\server");
        assert_eq!(value["syntheticIdentity"]["enabled"], true);
    }

    #[test]
    fn patch_can_set_clear_and_toggle_fields() {
        let mut config = AppConfig::default();

        config.apply_patch(ConfigPatch {
            server_project: Some(Some(PathBuf::from(r"D:\server"))),
            synthetic_identity: Some(SyntheticIdentityPatch {
                enabled: Some(true),
            }),
            ..ConfigPatch::default()
        });

        assert_eq!(config.server_project, Some(PathBuf::from(r"D:\server")));
        assert!(config.synthetic_identity.enabled);

        config.apply_patch(ConfigPatch {
            server_project: Some(None),
            ..ConfigPatch::default()
        });

        assert_eq!(config.server_project, None);
    }

    #[test]
    fn load_or_create_persists_default_config() {
        let store = test_store("create-default");
        let config = store.load_or_create().expect("create config");

        assert_eq!(config, AppConfig::default());
        assert!(store.path().is_file());

        let reloaded = store.load().expect("reload config");
        assert_eq!(reloaded, config);
    }

    #[test]
    fn save_and_load_round_trip() {
        let store = test_store("round-trip");
        let config = AppConfig {
            server_project: Some(PathBuf::from(r"D:\server")),
            fxserver_path: Some(PathBuf::from(r"D:\cfx\FXServer.exe")),
            fivem_path: Some(PathBuf::from(r"C:\FiveM\FiveM.exe")),
            synthetic_identity: SyntheticIdentityConfig { enabled: true },
            ..AppConfig::default()
        };

        store.save(&config).expect("save config");
        assert_eq!(store.load().expect("load config"), config);
    }

    #[test]
    fn unsupported_schema_is_rejected() {
        let store = test_store("unsupported-schema");
        let parent = store.path().parent().expect("config parent");
        fs::create_dir_all(parent).expect("create config parent");
        fs::write(
            store.path(),
            br#"{
  "schemaVersion": 999,
  "serverProject": null,
  "fxserverPath": null,
  "fivemPath": null,
  "syntheticIdentity": { "enabled": false }
}"#,
        )
        .expect("write unsupported config");

        let error = store.load().expect_err("schema must fail");
        assert!(matches!(
            error,
            ConfigStoreError::UnsupportedSchema {
                found: 999,
                supported: CONFIG_SCHEMA_VERSION
            }
        ));
    }

    #[test]
    fn runtime_path_validation_accepts_existing_directory_and_files() {
        let store = test_store("valid-paths");
        let root = store.path().parent().expect("test root");
        let server_project = root.join("server");
        let fxserver = root.join("FXServer.exe");
        let fivem = root.join("FiveM.exe");

        fs::create_dir_all(&server_project).expect("create server project");
        fs::write(&fxserver, b"test").expect("create fxserver");
        fs::write(&fivem, b"test").expect("create fivem");

        let config = AppConfig {
            server_project: Some(server_project),
            fxserver_path: Some(fxserver),
            fivem_path: Some(fivem),
            ..AppConfig::default()
        };

        assert!(validate_runtime_paths(&config).is_empty());
    }

    #[test]
    fn runtime_path_validation_reports_field_specific_issues() {
        let store = test_store("invalid-paths");
        let root = store.path().parent().expect("test root");
        fs::create_dir_all(root).expect("create test root");

        let fxserver_directory = root.join("FXServer.exe");
        fs::create_dir_all(&fxserver_directory).expect("create fake executable directory");

        let config = AppConfig {
            server_project: Some(root.join("missing-server")),
            fxserver_path: Some(fxserver_directory),
            fivem_path: Some(root.join("missing-FiveM.exe")),
            ..AppConfig::default()
        };

        let issues = validate_runtime_paths(&config);

        assert!(issues.iter().any(|issue| {
            issue.field == "serverProject" && issue.code == "PATH_NOT_FOUND"
        }));
        assert!(issues.iter().any(|issue| {
            issue.field == "fxserverPath" && issue.code == "NOT_A_FILE"
        }));
        assert!(issues.iter().any(|issue| {
            issue.field == "fivemPath" && issue.code == "PATH_NOT_FOUND"
        }));
    }
}
