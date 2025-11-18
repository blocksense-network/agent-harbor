// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Event handling for the TUI application

use crossterm::event::{self, Event as CrosstermEvent, KeyCode, KeyEvent, KeyModifiers};
use futures::FutureExt;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Events that can occur in the TUI application
#[derive(Debug, Clone)]
pub enum Event {
    /// Terminal input event (keyboard, mouse, etc.)
    Input(CrosstermEvent),
    /// Tick event for periodic updates
    Tick,
    /// Application should quit
    Quit,
    /// Error occurred
    Error(String),
}

/// Event handler for managing the event loop
pub struct EventHandler {
    sender: mpsc::UnboundedSender<Event>,
    receiver: mpsc::UnboundedReceiver<Event>,
    cancellation_token: CancellationToken,
}

impl EventHandler {
    /// Create a new event handler
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();
        let cancellation_token = CancellationToken::new();

        Self {
            sender,
            receiver,
            cancellation_token,
        }
    }

    /// Start the event handling loop
    pub async fn run(&mut self) {
        let sender = self.sender.clone();
        let cancellation_token = self.cancellation_token.clone();

        tokio::spawn(async move {
            Self::event_loop(sender, cancellation_token).await;
        });
    }

    /// Get the next event
    pub async fn next(&mut self) -> Option<Event> {
        self.receiver.recv().await
    }

    /// Send a quit event
    pub fn quit(&self) {
        let _ = self.sender.send(Event::Quit);
    }

    /// Cancel the event loop
    pub fn cancel(&self) {
        self.cancellation_token.cancel();
    }

    async fn event_loop(
        sender: mpsc::UnboundedSender<Event>,
        cancellation_token: CancellationToken,
    ) {
        let mut tick_interval = tokio::time::interval(Duration::from_millis(100));

        loop {
            if cancellation_token.is_cancelled() {
                break;
            }

            let tick_delay = tick_interval.tick();
            let crossterm_event = futures::future::ready(event::read()).fuse();

            tokio::select! {
                _ = cancellation_token.cancelled() => {
                    break;
                }
                _ = tick_delay => {
                    if sender.send(Event::Tick).is_err() {
                        break;
                    }
                }
                event = crossterm_event => {
                    match event {
                        Ok(evt) => {
                            if let Event::Input(ref input) = Event::Input(evt.clone()) {
                                if Self::should_quit(input) {
                                    let _ = sender.send(Event::Quit);
                                    break;
                                }
                            }

                            if sender.send(Event::Input(evt)).is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            let _ = sender.send(Event::Error(e.to_string()));
                            break;
                        }
                    }
                }
            }
        }
    }

    fn should_quit(event: &CrosstermEvent) -> bool {
        matches!(
            event,
            CrosstermEvent::Key(KeyEvent {
                code: KeyCode::Char('c'),
                modifiers: KeyModifiers::CONTROL,
                ..
            }) | CrosstermEvent::Key(KeyEvent {
                code: KeyCode::Char('q'),
                ..
            }) | CrosstermEvent::Key(KeyEvent {
                code: KeyCode::Esc,
                ..
            })
        )
    }
}

impl Default for EventHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_quit() {
        // Ctrl+C should quit
        let ctrl_c = CrosstermEvent::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(EventHandler::should_quit(&ctrl_c));

        // 'q' should quit
        let q = CrosstermEvent::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        assert!(EventHandler::should_quit(&q));

        // Escape should quit
        let esc = CrosstermEvent::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(EventHandler::should_quit(&esc));

        // Other keys should not quit
        let a = CrosstermEvent::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        assert!(!EventHandler::should_quit(&a));
    }
}
