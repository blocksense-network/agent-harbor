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
    /// Scenarios lacking an effective initial prompt are ignored unless no other
    /// candidates exist.
    pub fn best_match(&self, prompt: &str) -> Option<MatchedScenario<'a>> {
        let mut best: Option<MatchedScenario> = None;
        let mut first_fallback: Option<&Scenario> = None;
        for record in self.scenarios {
            if first_fallback.is_none() {
                first_fallback = Some(&record.scenario);
            }

            if let Some(reference) = record.scenario.effective_initial_prompt() {
                let distance = levenshtein(reference.as_str(), prompt);
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

        best.or_else(|| {
            first_fallback.map(|scenario| MatchedScenario {
                scenario,
                distance: 0,
            })
        })
    }
}
