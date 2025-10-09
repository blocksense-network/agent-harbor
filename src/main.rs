use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph, Wrap},
    Frame, Terminal,
};
use std::io;
use std::time::{Duration, Instant};
use rand::seq::SliceRandom;

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
