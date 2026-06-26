use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

pub const CONFIG_FILE: &str = ".uplate.jsonc";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UplateConfig {
    pub schema_version: u32,
    pub source: SourceConfig,
    pub base: BaseConfig,
    pub current: CurrentConfig,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceConfig {
    #[serde(rename = "type")]
    pub kind: SourceType,
    pub input: String,
    pub remote: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(rename = "ref")]
    pub ref_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SourceType {
    Github,
    Git,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BaseConfig {
    pub commit: String,
    #[serde(rename = "ref")]
    pub ref_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentConfig {
    pub commit: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upgraded_at: Option<DateTime<Utc>>,
}

impl UplateConfig {
    pub fn read_from(project_root: &Path) -> Result<Self> {
        let path = project_root.join(CONFIG_FILE);
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let stripped = json_comments::StripComments::new(raw.as_bytes());
        serde_json::from_reader(stripped)
            .with_context(|| format!("failed to parse {}", path.display()))
    }

    pub fn write_to(&self, project_root: &Path) -> Result<()> {
        let path = project_root.join(CONFIG_FILE);
        let json = serde_json::to_string_pretty(self)?;
        fs::write(&path, format!("{json}\n"))
            .with_context(|| format!("failed to write {}", path.display()))
    }
}
