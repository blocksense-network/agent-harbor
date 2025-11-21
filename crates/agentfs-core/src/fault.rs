// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Fault injection policy + runtime controller for FsCore

use crate::FsError;
use libc::EIO;
use serde::{Deserialize, Serialize};
use std::io;
use std::sync::Mutex;

/// Supported operations for storage-level fault injection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FaultOp {
    Read,
    Write,
    Truncate,
    Allocate,
    CloneCow,
    Sync,
}

/// Supported errno values for synthetic failures.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FaultErrno {
    Eio,
    Enospc,
}

impl FaultErrno {
    fn to_error(self) -> FsError {
        match self {
            FaultErrno::Eio => FsError::Io(io::Error::from_raw_os_error(EIO)),
            FaultErrno::Enospc => FsError::NoSpace,
        }
    }
}

/// Individual rule describing which op should fail and how often.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FaultRule {
    pub op: FaultOp,
    pub errno: FaultErrno,
    /// Optional number of leading invocations to skip before injecting faults.
    #[serde(default)]
    pub start_after: u64,
    /// Optional maximum number of injected failures for this rule.
    #[serde(default)]
    pub max_faults: Option<u64>,
}

impl Default for FaultRule {
    fn default() -> Self {
        Self {
            op: FaultOp::Write,
            errno: FaultErrno::Eio,
            start_after: 0,
            max_faults: None,
        }
    }
}

/// JSON-serializable policy transmitted over the control plane.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct FaultPolicy {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub rules: Vec<FaultRule>,
}

impl FaultPolicy {
    pub fn from_json_bytes(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }

    pub fn summary(&self) -> FaultPolicySummary {
        FaultPolicySummary {
            enabled: self.enabled,
            active: self.enabled && !self.rules.is_empty(),
            rule_count: self.rules.len(),
        }
    }
}

/// Lightweight summary returned to callers.
#[derive(Clone, Debug, Default)]
pub struct FaultPolicySummary {
    pub enabled: bool,
    pub active: bool,
    pub rule_count: usize,
}

#[derive(Clone, Debug, Default)]
struct RuleCounters {
    hits: u64,
    invocations: u64,
}

#[derive(Clone, Debug, Default)]
struct FaultState {
    policy: FaultPolicy,
    counters: Vec<RuleCounters>,
}

/// Runtime controller that tracks policy + hit counts.
pub struct FaultInjector {
    state: Mutex<FaultState>,
}

impl Default for FaultInjector {
    fn default() -> Self {
        Self::new()
    }
}

impl FaultInjector {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(FaultState::default()),
        }
    }

    pub fn snapshot(&self) -> FaultPolicy {
        self.state.lock().unwrap().policy.clone()
    }

    pub fn summary(&self) -> FaultPolicySummary {
        self.snapshot().summary()
    }

    pub fn set_policy(&self, policy: FaultPolicy) {
        let mut guard = self.state.lock().unwrap();
        guard.counters = vec![RuleCounters::default(); policy.rules.len()];
        guard.policy = policy;
    }

    pub fn clear(&self) {
        self.set_policy(FaultPolicy::default());
    }

    pub fn should_fault(&self, op: FaultOp) -> Option<FsError> {
        let mut guard = self.state.lock().unwrap();
        if !guard.policy.enabled {
            return None;
        }
        let rules_len = guard.policy.rules.len();
        if guard.counters.len() < rules_len {
            guard.counters.resize(rules_len, RuleCounters::default());
        }
        for idx in 0..rules_len {
            let rule = guard.policy.rules[idx].clone();
            if rule.op != op {
                continue;
            }
            let counters =
                guard.counters.get_mut(idx).unwrap_or_else(|| unreachable!("counters aligned"));
            counters.invocations = counters.invocations.saturating_add(1);
            if counters.invocations <= rule.start_after {
                continue;
            }
            if let Some(max) = rule.max_faults {
                if counters.hits >= max {
                    continue;
                }
            }
            counters.hits = counters.hits.saturating_add(1);
            return Some(rule.errno.to_error());
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fault_policy_json() {
        let json = br#"{ "enabled": true, "rules": [ { "op": "write", "errno": "eio", "max_faults": 2 } ] }"#;
        let policy = FaultPolicy::from_json_bytes(json).expect("policy");
        assert!(policy.enabled);
        assert_eq!(policy.rules.len(), 1);
        assert_eq!(policy.rules[0].max_faults, Some(2));
    }

    #[test]
    fn injector_respects_start_and_max_hits() {
        let injector = FaultInjector::new();
        injector.set_policy(FaultPolicy {
            enabled: true,
            rules: vec![FaultRule {
                op: FaultOp::Write,
                errno: FaultErrno::Eio,
                start_after: 1,
                max_faults: Some(2),
            }],
        });

        // First call skipped due to start_after
        assert!(injector.should_fault(FaultOp::Write).is_none());
        // Next two fail
        assert!(injector.should_fault(FaultOp::Write).is_some());
        assert!(injector.should_fault(FaultOp::Write).is_some());
        // Max hits reached
        assert!(injector.should_fault(FaultOp::Write).is_none());
    }
}
