#!/usr/bin/env bash

# Run the TUI exploration application
# This displays the interactive dashboard with Charm theming

echo "🎨 Running TUI Exploration - Interactive Dashboard with Charm Theme"
echo ""
echo "✨ Features:"
echo "  • Graphical PNG logo (with ASCII fallback)"
echo "  • Charm-inspired Catppuccin Mocha theme"
echo "  • Direct text editing (no Enter required)"
echo "  • Real-time activity simulation"
echo "  • Fuzzy search modals"
echo "  • Full keyboard navigation"
echo ""
echo "🎯 Try:"
echo "  • ↑↓ to navigate between task cards"
echo "  • Enter on draft card to start typing immediately"
echo "  • Tab to cycle through buttons"
echo "  • Enter on buttons to open fuzzy search"
echo "  • Watch active task for live activity updates"
echo ""
echo "Press Esc to exit"
echo ""

cd "$(dirname "$0")"
cargo run --release
