// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use rand::seq::SliceRandom;
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph, Wrap},
};
use std::io;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq)]
enum TaskState {
    Draft,
    Active,
    Completed,
}

#[derive(Debug, Clone)]
struct TaskCard {
    title: String,
    repository: String,
    branch: String,
    agent: String,
    timestamp: String,
    state: TaskState,
    activity: Vec<String>,
    delivery_indicators: Option<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("TUI Exploration - Minimal test");
    Ok(())
}
