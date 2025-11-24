// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Scenario-Format parser and playback utilities shared across Agent Harbor components.

mod error;
mod loader;
mod matching;
mod model;
mod playback;

pub use error::{Result, ScenarioError};
pub use loader::{ScenarioLoader, ScenarioRecord, ScenarioSource};
pub use matching::{MatchedScenario, ScenarioMatcher};
pub use model::*;
pub use playback::{
    PlaybackEvent, PlaybackEventKind, PlaybackIterator, PlaybackOptions, TimelinePosition,
};

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use tempfile::tempdir;

    use super::*;

    const SAMPLE: &str = r#"
name: demo
initialPrompt: "Fix the failing tests"
timeline:
  - think:
      - [100, "Looking into the issue"]
  - agentToolUse:
      toolName: runCmd
      args:
        cmd: "npm test"
      progress:
        - [200, "Running tests"]
      result: "All tests passed"
      status: "ok"
  - advanceMs: 500
  - assistant:
      - [100, "All good now"]
  - complete: true
"#;

    const CONTROL: &str = r#"
name: control
initialPrompt: "Demonstrate control events"
timeline:
  - think:
      - [100, "Booting"]
  - type: advanceMs
    ms: 900
  - userInputs:
      - [100, "hello world"]
    target: tui
  - assistant:
      - [100, "Response after a second"]
  - assert:
      text:
        contains:
          - "Done"
  - complete: true
"#;

    #[test]
    fn load_and_iterate_scenario() {
        let scenario: Scenario = serde_yaml::from_str(SAMPLE).expect("parse scenario");
        assert_eq!(scenario.name, "demo");
        assert_eq!(
            scenario.initial_prompt.as_deref(),
            Some("Fix the failing tests")
        );
        assert_eq!(scenario.timeline.len(), 5);

        let iterator = PlaybackIterator::new(&scenario, PlaybackOptions::default())
            .expect("build playback iterator");
        let events: Vec<_> = iterator.collect();
        assert!(events.iter().any(|e| matches!(e.kind, PlaybackEventKind::Thinking { .. })));
        assert!(events.iter().any(|e| matches!(e.kind, PlaybackEventKind::ToolStart { .. })));
        assert!(events.iter().any(|e| matches!(e.kind, PlaybackEventKind::Complete)));
    }

    #[test]
    fn loader_reads_directory_sources() {
        let dir = tempdir().expect("tempdir");
        let file_path = dir.path().join("demo.yaml");
        fs::write(&file_path, SAMPLE).expect("write sample");

        let loader =
            ScenarioLoader::from_sources([ScenarioSource::Directory(dir.path().to_path_buf())])
                .expect("load");
        assert_eq!(loader.scenarios().len(), 1);
        assert_eq!(loader.scenarios()[0].scenario.name, "demo");
        assert_eq!(loader.scenarios()[0].path, file_path);
    }

    #[test]
    fn matcher_prefers_closest_prompt() {
        let scenario_a: Scenario = serde_yaml::from_str(SAMPLE).unwrap();
        let mut scenario_b = scenario_a.clone();
        scenario_b.name = "other".into();
        scenario_b.initial_prompt = Some("Implement OAuth login".into());

        let records = vec![
            ScenarioRecord {
                scenario: scenario_a,
                path: PathBuf::from("a.yaml"),
            },
            ScenarioRecord {
                scenario: scenario_b,
                path: PathBuf::from("b.yaml"),
            },
        ];

        let matcher = ScenarioMatcher::new(&records);
        let matched = matcher.best_match("Please fix failing unit tests").expect("match scenario");
        assert_eq!(matched.scenario.name, "demo");
    }

    #[test]
    fn matcher_falls_back_when_no_initial_prompts() {
        let mut scenario_a: Scenario = serde_yaml::from_str(SAMPLE).unwrap();
        scenario_a.initial_prompt = None;
        let records = vec![ScenarioRecord {
            scenario: scenario_a.clone(),
            path: PathBuf::from("a.yaml"),
        }];
        let matcher = ScenarioMatcher::new(&records);
        let matched = matcher.best_match("anything").expect("match");
        assert_eq!(matched.scenario.name, scenario_a.name);
    }

    #[test]
    fn playback_speed_adjusts_schedule() {
        let scenario: Scenario = serde_yaml::from_str(SAMPLE).unwrap();
        let fast = PlaybackIterator::new(
            &scenario,
            PlaybackOptions {
                speed_multiplier: 0.1,
            },
        )
        .unwrap();
        let slow = PlaybackIterator::new(
            &scenario,
            PlaybackOptions {
                speed_multiplier: 2.0,
            },
        )
        .unwrap();

        let fast_last = fast.last().unwrap();
        let slow_last = slow.last().unwrap();
        assert!(fast_last.at_ms < slow_last.at_ms);
    }

    #[test]
    fn playback_honors_control_and_user_events() {
        let scenario: Scenario = serde_yaml::from_str(CONTROL).unwrap();
        let events: Vec<_> =
            PlaybackIterator::new(&scenario, PlaybackOptions::default()).unwrap().collect();
        let mut saw_user_input = false;
        let mut saw_assert = false;
        for event in &events {
            match &event.kind {
                PlaybackEventKind::UserInput { .. } => saw_user_input = true,
                PlaybackEventKind::Assert(_) => saw_assert = true,
                _ => {}
            }
        }
        assert!(saw_user_input, "userInputs should emit playback events");
        assert!(saw_assert, "assert blocks should emit playback events");
    }
}
