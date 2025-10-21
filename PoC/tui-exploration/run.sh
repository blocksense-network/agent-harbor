#!/usr/bin/env bash

# Run the TUI exploration application
# This displays the interactive dashboard with Charm theming

# Parse command line arguments
RAW_MODE_FLAG=""
for arg in "$@"; do
  case $arg in
  --no-raw-mode)
    RAW_MODE_FLAG="--no-raw-mode"
    shift
    ;;
  --help | -h)
    echo "🎨 TUI Exploration - Interactive Dashboard with Charm Theme"
    echo ""
    echo "Usage: $0 [OPTIONS]"
    echo ""
    echo "Options:"
    echo "  --no-raw-mode    Disable raw mode (useful for debugging, disables keyboard input)"
    echo "  --help, -h       Show this help message"
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
    exit 0
    ;;
  *)
    echo "Unknown option: $arg"
    echo "Use --help for usage information"
    exit 1
    ;;
  esac
done

echo "🎨 Running TUI Exploration - Interactive Dashboard with Charm Theme"
if [ -n "$RAW_MODE_FLAG" ]; then
  echo "⚠️  Running in debug mode (--no-raw-mode): Keyboard input disabled"
fi
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
if [ -n "$RAW_MODE_FLAG" ]; then
  cargo run --release -- $RAW_MODE_FLAG
else
  cargo run --release
fi
