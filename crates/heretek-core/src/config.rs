use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::profile::ModelProfile;

pub const CONFIG_FILE: &str = ".heretek.toml";

pub fn validate_git_ref(value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err("git ref must not be empty".to_string());
    }
    if value.starts_with('-') {
        return Err(format!("git ref \"{value}\" must not start with '-'"));
    }
    if value
        .chars()
        .any(|character| character.is_whitespace() || character.is_control())
    {
        return Err(format!("git ref \"{value}\" must not contain whitespace"));
    }
    if value.contains("..") || value.contains('~') || value.contains('^') || value.contains(':') {
        return Err(format!("git ref \"{value}\" contains forbidden characters"));
    }
    Ok(())
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HereConfig {
    pub gate: GateConfig,
    pub agent: AgentConfig,
    pub models: BTreeMap<String, ModelProfile>,
    pub default_lane: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StageToggles {
    pub syntax: bool,
    pub format: bool,
    pub typecheck: bool,
    pub tests: bool,
    pub ast_grep: bool,
    pub secrets: bool,
    pub sast: bool,
    pub dead_code: bool,
    pub deps: bool,
}

impl Default for StageToggles {
    fn default() -> Self {
        Self {
            syntax: true,
            format: true,
            typecheck: true,
            tests: true,
            ast_grep: true,
            secrets: true,
            sast: true,
            dead_code: true,
            deps: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GateConfig {
    pub stages: StageToggles,
    pub baseline: Option<String>,
    pub stage_timeout_secs: u64,
    pub max_output_bytes: usize,
    pub allow_network: bool,
    pub ast_grep_rules: Option<PathBuf>,
    pub semgrep_config: Option<String>,
    pub ignore: Vec<String>,
}

impl Default for GateConfig {
    fn default() -> Self {
        Self {
            stages: StageToggles::default(),
            baseline: None,
            stage_timeout_secs: 300,
            max_output_bytes: 1_048_576,
            allow_network: false,
            ast_grep_rules: None,
            semgrep_config: None,
            ignore: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AgentConfig {
    pub max_turns: u32,
    pub max_wall_secs: u64,
    pub escalate: bool,
    pub tool_result_token_budget: usize,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_turns: 40,
            max_wall_secs: 1800,
            escalate: true,
            tool_result_token_budget: 3000,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("cannot read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("invalid configuration: {0}")]
    Invalid(String),
}

impl HereConfig {
    pub fn load(repo_root: &Path) -> Result<Self, ConfigError> {
        let path = repo_root.join(CONFIG_FILE);
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(&path).map_err(|source| ConfigError::Read {
            path: path.clone(),
            source,
        })?;
        let config: Self = toml::from_str(&text).map_err(|source| ConfigError::Parse {
            path: path.clone(),
            source,
        })?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.gate.stage_timeout_secs == 0 {
            return Err(ConfigError::Invalid(
                "gate.stage_timeout_secs must be greater than zero".to_string(),
            ));
        }
        if self.agent.max_turns == 0 {
            return Err(ConfigError::Invalid(
                "agent.max_turns must be greater than zero".to_string(),
            ));
        }
        if let Some(baseline) = &self.gate.baseline {
            validate_git_ref(baseline).map_err(ConfigError::Invalid)?;
        }
        for (name, profile) in &self.models {
            if profile.base_url.trim().is_empty() {
                return Err(ConfigError::Invalid(format!(
                    "models.{name}.base_url must not be empty"
                )));
            }
            if profile.model.trim().is_empty() {
                return Err(ConfigError::Invalid(format!(
                    "models.{name}.model must not be empty"
                )));
            }
        }
        if let Some(lane) = &self.default_lane
            && !self.models.contains_key(lane)
        {
            return Err(ConfigError::Invalid(format!(
                "default_lane \"{lane}\" has no matching models entry"
            )));
        }
        Ok(())
    }
}
