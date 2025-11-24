// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use crate::{Scenario, loader::ScenarioRecord};
use strsim::levenshtein;

/// Result of fuzzy matching a prompt to scenarios.
#[derive(Debug, Clone)]
pub struct MatchedScenario<'a> {
    pub scenario: &'a Scenario,
    pub distance: usize,
}

/// Matches prompts to scenarios using Levenshtein distance on `initialPrompt`.
#[derive(Debug)]
pub struct ScenarioMatcher<'a> {
    scenarios: &'a [ScenarioRecord],
}

impl<'a> ScenarioMatcher<'a> {
    pub fn new(scenarios: &'a [ScenarioRecord]) -> Self {
        Self { scenarios }
    }

    /// Returns the scenario with the smallest Levenshtein distance to the prompt.
    /// Scenarios lacking `initialPrompt` are ignored unless no other candidates exist.
    pub fn best_match(&self, prompt: &str) -> Option<MatchedScenario<'a>> {
        let mut best: Option<MatchedScenario> = None;
        for record in self.scenarios {
            if let Some(reference) = record.scenario.initial_prompt.as_deref() {
                let distance = levenshtein(reference, prompt);
                match &mut best {
                    Some(current) if distance < current.distance => {
                        *current = MatchedScenario {
                            scenario: &record.scenario,
                            distance,
                        };
                    }
                    None => {
                        best = Some(MatchedScenario {
                            scenario: &record.scenario,
                            distance,
                        });
                    }
                    _ => {}
                }
            }
        }

        if best.is_some() {
            return best;
        }

        // No scenarios had initial prompts; fall back to the first scenario.
        self.scenarios.first().map(|record| MatchedScenario {
            scenario: &record.scenario,
            distance: 0,
        })
    }
}
