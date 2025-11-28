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
  - llmResponse:
      - think:
          - relativeTime: 100
            content: "Looking into the issue"
  - agentToolUse:
      toolName: runCmd
      args:
        cmd: "npm test"
      progress:
        - relativeTime: 200
          content: "Running tests"
      result: "All tests passed"
      status: "ok"
  - baseTimeDelta: 500
  - llmResponse:
      - assistant:
          - relativeTime: 100
            content: "All good now"
  - complete: true
"#;

    const CONTROL: &str = r#"
name: control
initialPrompt: "Demonstrate control events"
timeline:
  - llmResponse:
      - think:
          - relativeTime: 100
            content: "Booting"
  - baseTimeDelta: 900
  - userInputs:
      - relativeTime: 1000
        input: "hello world"
        target: tui
  - llmResponse:
      - assistant:
          - relativeTime: 100
            content: "Response after a second"
  - assert:
      text:
        contains:
          - "Done"
  - complete: true
"#;

    const NEW_USER_INPUTS: &str = r#"
name: new_user_inputs
timeline:
  - baseTimeDelta: 100
  - userInputs:
      - relativeTime: 150
        input: "hello modern world"
        target: "tui"
  - complete: true
"#;

    const INVALID_USER_INPUTS: &str = r#"
name: invalid_user_inputs
timeline:
  - llmResponse:
      - think:
          - relativeTime: 100
            content: "wait"
  - userInputs:
      - relativeTime: 50
        input: "too early"
"#;

    const USER_INPUT_PROMPT: &str = r#"
name: user_prompt_demo
timeline:
  - userInputs:
      - relativeTime: 0
        input: "before boundary"
  - baseTimeDelta: 0
  - userInputs:
      - relativeTime: 0
        input: "after boundary"
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
    fn effective_initial_prompt_prefers_post_boundary_user_inputs() {
        let scenario: Scenario = serde_yaml::from_str(USER_INPUT_PROMPT).unwrap();
        assert_eq!(
            scenario.effective_initial_prompt().as_deref(),
            Some("before boundary")
        );
    }

    #[test]
    fn matcher_uses_effective_prompt_when_available() {
        let scenario: Scenario = serde_yaml::from_str(USER_INPUT_PROMPT).unwrap();
        let records = vec![ScenarioRecord {
            scenario,
            path: PathBuf::from("a.yaml"),
        }];

        let matcher = ScenarioMatcher::new(&records);
        let matched = matcher.best_match("after boundary").expect("match scenario");
        assert_eq!(matched.scenario.name, "user_prompt_demo");
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
        if !saw_user_input || !saw_assert {
            let kinds: Vec<_> = events.iter().map(|e| format!("{:?}", e.kind)).collect();
            assert!(
                saw_user_input,
                "userInputs should emit playback events: {:?}",
                kinds
            );
            assert!(
                saw_assert,
                "assert blocks should emit playback events: {:?}",
                kinds
            );
        }
    }

    #[test]
    fn playback_supports_object_user_inputs() {
        let scenario: Scenario = serde_yaml::from_str(NEW_USER_INPUTS).unwrap();
        let events: Vec<_> =
            PlaybackIterator::new(&scenario, PlaybackOptions::default()).unwrap().collect();
        let mut user_at = None;
        for event in &events {
            if let PlaybackEventKind::UserInput { value, target } = &event.kind {
                user_at = Some((event.at_ms, value.clone(), target.clone()));
            }
        }
        let (at_ms, value, target) = user_at.expect("user input emitted");
        assert_eq!(at_ms, 150);
        assert_eq!(value, "hello modern world");
        assert_eq!(target.as_deref(), Some("tui"));
    }

    #[test]
    fn playback_rejects_non_monotonic_user_inputs() {
        let scenario: Scenario = serde_yaml::from_str(INVALID_USER_INPUTS).unwrap();
        let err = PlaybackIterator::new(&scenario, PlaybackOptions::default());
        match err {
            Ok(_) => panic!("expected playback to fail"),
            Err(e) => {
                let msg = format!("{}", e);
                assert!(
                    msg.contains("userInputs relativeTime 50 is earlier"),
                    "unexpected error message: {msg}"
                );
            }
        }
    }
}
