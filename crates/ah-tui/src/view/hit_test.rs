// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use ratatui::layout::Rect;
use tracing::debug;

/// Represents an interactive zone on screen associated with a semantic action.
#[derive(Debug, Clone)]
pub struct HitZone<A> {
    pub rect: Rect,
    pub action: A,
}

/// Result of a hit-test lookup containing the matched action and its screen bounds.
#[derive(Debug, Clone)]
pub struct HitMatch<A> {
    pub rect: Rect,
    pub action: A,
}

/// Collector that accumulates hit-test zones during rendering and resolves them on demand.
#[derive(Debug, Default)]
pub struct HitTestRegistry<A> {
    zones: Vec<HitZone<A>>,
}

impl<A> HitTestRegistry<A> {
    pub fn new() -> Self {
        Self { zones: Vec::new() }
    }

    /// Remove all registered zones so the collector can be reused for the next frame.
    pub fn clear(&mut self) {
        self.zones.clear();
    }

    /// Register a new interactive zone together with its semantic action.
    pub fn register(&mut self, rect: Rect, action: A) {
        self.zones.push(HitZone { rect, action });
    }
}

impl<A: Clone> HitTestRegistry<A> {
    /// Perform a hit test at the provided terminal coordinates and return the top-most action.
    pub fn hit_test(&self, column: u16, row: u16) -> Option<HitMatch<A>> {
        self.zones.iter().rev().find_map(|zone| {
            if rect_contains(zone.rect, column, row) {
                Some(HitMatch {
                    rect: zone.rect,
                    action: zone.action.clone(),
                })
            } else {
                None
            }
        })
    }

    /// Get the number of registered zones.
    pub fn len(&self) -> usize {
        self.zones.len()
    }

    /// Returns true if there are no registered zones.
    pub fn is_empty(&self) -> bool {
        self.zones.is_empty()
    }

    /// Debug dump all registered zones.
    pub fn debug_dump(&self)
    where
        A: std::fmt::Debug,
    {
        for (i, zone) in self.zones.iter().enumerate() {
            debug!("  Zone {}: {:?} at {:?}", i, zone.action, zone.rect);
        }
    }
}

fn rect_contains(rect: Rect, column: u16, row: u16) -> bool {
    column >= rect.x
        && row >= rect.y
        && column < rect.x.saturating_add(rect.width)
        && row < rect.y.saturating_add(rect.height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Action {
        One,
        Two,
    }

    #[test]
    fn returns_latest_registered_zone_first() {
        let mut registry = HitTestRegistry::new();
        registry.register(
            Rect {
                x: 0,
                y: 0,
                width: 10,
                height: 5,
            },
            Action::One,
        );
        registry.register(
            Rect {
                x: 0,
                y: 0,
                width: 5,
                height: 2,
            },
            Action::Two,
        );

        let result = registry.hit_test(2, 1).expect("should hit top zone");
        assert_eq!(result.action, Action::Two);
    }

    #[test]
    fn returns_none_when_no_zone_matches() {
        let mut registry = HitTestRegistry::new();
        registry.register(
            Rect {
                x: 0,
                y: 0,
                width: 2,
                height: 2,
            },
            Action::One,
        );

        assert!(registry.hit_test(10, 10).is_none());
    }
}
