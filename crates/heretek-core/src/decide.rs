use crate::profile::Lane;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RiskTier {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decision<T> {
    pub value: T,
    pub reason: &'static str,
}

impl<T> Decision<T> {
    pub fn new(value: T, reason: &'static str) -> Self {
        Self { value, reason }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChangeSummary {
    pub paths: Vec<String>,
    pub files_changed: usize,
    pub lines_changed: usize,
    pub touches_tests: bool,
    pub touches_dependencies: bool,
}

impl ChangeSummary {
    pub fn touches_any(&self, needles: &[&str]) -> bool {
        self.paths.iter().any(|path| {
            let lowered = path.to_ascii_lowercase();
            needles.iter().any(|needle| lowered.contains(needle))
        })
    }
}

pub trait Decider {
    fn risky_change(&self, change: &ChangeSummary) -> Decision<RiskTier>;
    fn lane_for(&self, risk: RiskTier) -> Decision<Lane>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct HeuristicDecider;

const HIGH_RISK_PATHS: &[&str] = &[
    "auth",
    "migration",
    "migrations",
    ".github/",
    "dockerfile",
    "lock",
    "ci/",
    "infra",
];

impl Decider for HeuristicDecider {
    fn risky_change(&self, change: &ChangeSummary) -> Decision<RiskTier> {
        if change.touches_dependencies {
            return Decision::new(RiskTier::High, "dependency change");
        }
        if change.touches_any(HIGH_RISK_PATHS) {
            return Decision::new(RiskTier::High, "high-risk path touched");
        }
        if change.lines_changed > 400 || change.files_changed > 20 {
            return Decision::new(RiskTier::High, "large change");
        }
        if change.lines_changed < 50 && change.touches_tests {
            return Decision::new(RiskTier::Low, "small change with test coverage");
        }
        Decision::new(RiskTier::Medium, "unclassified change")
    }

    fn lane_for(&self, risk: RiskTier) -> Decision<Lane> {
        match risk {
            RiskTier::Low | RiskTier::Medium => Decision::new(Lane::Fast, "risk within fast lane"),
            RiskTier::High => Decision::new(Lane::Deep, "high risk escalates to deep lane"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(paths: &[&str], lines: usize, tests: bool, deps: bool) -> ChangeSummary {
        ChangeSummary {
            paths: paths.iter().map(|p| (*p).to_string()).collect(),
            files_changed: paths.len(),
            lines_changed: lines,
            touches_tests: tests,
            touches_dependencies: deps,
        }
    }

    #[test]
    fn small_tested_change_is_low_risk() {
        let decider = HeuristicDecider;
        let decision = decider.risky_change(&change(&["src/util/strings.ts"], 12, true, false));
        assert_eq!(decision.value, RiskTier::Low);
    }

    #[test]
    fn auth_path_is_high_risk() {
        let decider = HeuristicDecider;
        let decision = decider.risky_change(&change(&["src/auth/session.ts"], 10, true, false));
        assert_eq!(decision.value, RiskTier::High);
    }

    #[test]
    fn dependency_change_is_high_risk() {
        let decider = HeuristicDecider;
        let decision = decider.risky_change(&change(&["package.json"], 4, false, true));
        assert_eq!(decision.value, RiskTier::High);
    }

    #[test]
    fn high_risk_routes_to_deep_lane() {
        let decider = HeuristicDecider;
        let decision = decider.lane_for(RiskTier::High);
        assert_eq!(decision.value, Lane::Deep);
    }
}
