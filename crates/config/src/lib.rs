use std::path::PathBuf;

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
    use std::path::PathBuf;

    use super::{
        AppConfig, CONFIG_SCHEMA_VERSION, ConfigPatch, SyntheticIdentityConfig,
        SyntheticIdentityPatch,
    };

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
}
