use heretek_core::decide::Decider;
use heretek_core::{HereConfig, HeuristicDecider, Lane, RiskTier};

use crate::client::ModelClient;
use crate::error::ModelError;
use crate::types::ModelHealth;

pub struct Router {
    config: HereConfig,
}

impl Router {
    pub fn new(config: HereConfig) -> Self {
        Self { config }
    }

    pub fn client_for(&self, lane: &str) -> Result<ModelClient, ModelError> {
        let profile = self.config.models.get(lane).ok_or_else(|| {
            let available = self.config.models.keys().cloned().collect::<Vec<_>>();
            if available.is_empty() {
                ModelError::Config(format!(
                    "lane \"{lane}\" is not configured; no models configured"
                ))
            } else {
                ModelError::Config(format!(
                    "lane \"{lane}\" is not configured (available: {})",
                    available.join(", ")
                ))
            }
        })?;
        ModelClient::new(profile)
    }

    pub fn default_lane(&self) -> Result<&str, ModelError> {
        if let Some(lane) = &self.config.default_lane {
            return Ok(lane.as_str());
        }
        if self.config.models.contains_key("fast") {
            return Ok("fast");
        }
        self.config
            .models
            .keys()
            .next()
            .map(String::as_str)
            .ok_or_else(|| ModelError::Config("no models configured".to_string()))
    }

    pub fn lane_for_risk(&self, risk: RiskTier) -> &'static str {
        match HeuristicDecider.lane_for(risk).value {
            Lane::Deep => "deep",
            Lane::Fast | Lane::Micro => "fast",
        }
    }

    pub fn probe(&self) -> Vec<(String, ModelHealth)> {
        let mut results = self
            .config
            .models
            .iter()
            .map(|(key, profile)| {
                let health = match ModelClient::new(profile) {
                    Ok(client) => client.health(),
                    Err(error) => ModelHealth {
                        reachable: false,
                        models: Vec::new(),
                        latency_ms: 0,
                        error: Some(error.to_string()),
                    },
                };
                (key.clone(), health)
            })
            .collect::<Vec<_>>();
        results.sort_by(|left, right| left.0.cmp(&right.0));
        results
    }
}
