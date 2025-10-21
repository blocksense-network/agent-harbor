# TUI Exploration - Interactive Dashboard (T3.4 Enhanced)

This project implements **Milestone T3.4: Static Dashboard Rendering (Style Development)** for the Agent Harbor TUI, enhanced with full interactivity for comprehensive testing and design iteration.

## Overview

This is a fully interactive implementation that demonstrates the complete Agent Harbor TUI dashboard experience with **Charm-inspired theming**. It includes keyboard navigation, editable text input, fuzzy search modals, and simulated agent activity - everything needed to test the user experience before integrating with the full TUI application.

## Key Improvements

### 🎨 **Charm-Inspired Theme**

- **Catppuccin Mocha Colors**: Dark theme with cohesive color palette (base background, surface, text, primary/accent colors)
- **Rounded Borders**: `BorderType::Rounded` for modern Charm aesthetic
- **Graphical Logo**: PNG logo rendering with ASCII fallback using `ratatui-image`
- **Charm Title Borders**: `┤ Title ├` format blending with rounded corners
- **Proper Padding**: 1-character padding for tight, clean layout
- **Background Fills**: Theme-colored backgrounds for visual consistency

### 🎯 **Direct Text Editing**

- **Immediate Typing**: No need to press Enter first - just navigate to the description area and start typing
- **Cursor Management**: Proper cursor positioning and text insertion
- **Visual Feedback**: Blue highlighting when description area is focused
- **Line Breaks**: Shift+Enter for multi-line input

### ⚡ **Real-Time Activity Simulation**

- **Timer-Based Updates**: Every 2-5 seconds (randomized for realism)
- **Live Activity Feed**: Active tasks show continuous agent progress
- **Diverse Activities**: Thoughts, tool usage, file edits, command execution
- **3-Line Rolling Display**: Always shows exactly 3 most recent activities

## Features Implemented

### Interactive Keyboard Navigation

- **Arrow Keys**: Navigate between task cards (↑↓)
- **Tab/Shift+Tab**: Navigate between controls in draft cards
- **Enter**: Select cards, activate buttons, launch tasks
- **Esc**: Exit application or close modals
- **Visual Focus**: Selected cards and buttons highlighted with color changes

### Editable Task Description

- **Text Input**: Full text editing with cursor movement (←→)
- **Line Breaks**: Shift+Enter for new lines
- **Backspace**: Delete characters
- **Visual Feedback**: Blue background when focused

### Single-Line Controls Layout (PRD Compliant)

- **Repository Button**: Shows current selection, activates fuzzy search modal
- **Branch Button**: Shows current selection, activates fuzzy search modal
- **Model Button**: Shows current selection, activates fuzzy search modal
- **Go Button**: Launches task when description is filled

### Fuzzy Search Modals

- **Repository Selection**: Search through available repositories
- **Branch Selection**: Search through repository branches
- **Model Selection**: Search through available AI models
- **Live Filtering**: Type to filter options in real-time
- **Navigation**: ↑↓ to navigate, Enter to select, Esc to cancel
- **Centered Overlay**: Modal appears over main interface

### Simulated Agent Activity

- **Live Updates**: Active tasks show simulated agent activity every 3 seconds
- **Activity Types**: Thoughts, tool usage, file edits, status updates
- **3-Line Display**: Always shows exactly 3 most recent activities
- **Realistic Content**: Matches patterns from mock server scenarios

### Header

- **Agent Harbor Branding**: ASCII art logo with cyan coloring
- **Image Support Placeholder**: Ready for `ratatui-image` integration

### Task Cards

#### Draft Task Cards (Interactive)

- **Editable Description**: Full text input area with placeholder
- **Single-Line Controls**: Repository | Branch | Model | Go buttons
- **Button Focus**: Visual highlighting when navigating with Tab
- **Border Styling**: Cyan borders with focus indicators

#### Active Task Cards (Live Activity)

- **Status Indicator**: Yellow "●" bullet with bold styling
- **Live Activity Feed**: 3 fixed-height rows with scrolling updates
- **Simulated Actions**: Random activity every 3 seconds
- **Activity Types**:
  - Thoughts: "Analyzing codebase structure"
  - Tool usage: "grep", "run_terminal_cmd", etc.
  - File edits: "src/main.rs (+5 -2)"

#### Completed/Merged Task Cards (Static)

- **Status Indicator**: Green "✓" checkmark with bold styling
- **Delivery Indicators**: Unicode symbols (⎇ br ✓ ok)
- **Metadata**: Repository, branch, agent, timestamp

### Context-Sensitive Footer

- **Task Card Focus**: "↑↓ Navigate • Enter Select Task • Ctrl+C x2 Quit"
- **Description Focus**: "Enter Launch Agent(s) • Shift+Enter New Line • Tab Next Field"
- **Button Focus**: "↑↓ Navigate • Enter Select • Esc Back"
- **Go Button Focus**: "Enter Launch Task • Esc Back"

### Visual Design

- **Ratatui Styling**: Proper use of `Style`, `Color`, `Modifier` for visual hierarchy
- **Bordered Cards**: Rounded borders with state-appropriate colors
- **Typography**: Bold text for status indicators and buttons
- **Spacing**: Consistent padding and margins throughout
- **Color Scheme**: Cyan (draft), Yellow (active), Green (completed) with gray accents

## Technical Implementation

### Dependencies

- `ratatui = "0.29"` - Modern Rust TUI framework
- `crossterm = "0.27"` - Cross-platform terminal handling
- `ratatui-image = "8"` - Image rendering support (placeholder for future use)

### Architecture

- **TaskCard Struct**: Encapsulates all task data and rendering logic
- **TaskState Enum**: Draft, Active, Completed states with associated behaviors
- **Layout System**: Ratatui's Layout and Constraint system for responsive design
- **State-Aware Rendering**: Each card type renders differently based on its state

### Code Structure

```
src/
├── main.rs              # Main application logic and sample data
├── task rendering       # Individual card rendering methods
├── header/footer        # Branding and navigation components
└── layout management    # Responsive layout calculations
```

## Running the Application

```bash
# Option 1: Direct cargo run
cargo run --release

# Option 2: Use the convenience script
./run.sh
```

## Interactive Usage Guide

### Basic Navigation

- **↑↓**: Navigate between task cards
- **Enter**: Select a task card or activate focused button
- **Esc**: Exit application or close modals
- **Tab/Shift+Tab**: Navigate between controls in draft cards

### Task Creation Workflow

1. **Navigate to draft card**: Use ↑↓ to select the first (draft) task card
2. **Enter description**: Press Enter to focus the text area, then **start typing immediately**
3. **Edit text**: Use ←→ for cursor movement, Backspace to delete, Shift+Enter for new lines
4. **Configure options**: Use Tab to navigate to Repository/Branch/Model buttons
5. **Select options**: Press Enter on any button to open fuzzy search modal
6. **Launch task**: Use Tab to reach Go button, press Enter to launch

### Fuzzy Search Modals

- **Type to filter**: Start typing to filter options in real-time
- **Navigate options**: Use ↑↓ to move through filtered results
- **Select option**: Press Enter to choose the highlighted option
- **Cancel**: Press Esc to close modal without selecting

### Live Activity Observation

- **Active tasks**: Watch the second task card for simulated agent activity
- **Activity updates**: New activities appear every 3 seconds
- **Activity types**: Thoughts, tool usage, and file edits are simulated

### Visual Feedback

- **Graphical Logo**: PNG rendering with automatic ASCII fallback
- **Selected cards**: Theme-colored border with rounded corners
- **Focused text area**: Blue highlighting with Charm-style borders
- **Active buttons**: Primary/accent colors with bold styling
- **Modal overlays**: Shadow effects and rounded borders
- **Footer shortcuts**: Context-sensitive with theme colors
- **Live activity**: Real-time updates in active task cards

## Design Iteration

This implementation serves as a foundation for visual design iteration:

1. **Color Scheme**: Current cyan/yellow/green scheme can be adjusted
2. **Spacing**: Card padding and margins can be refined
3. **Typography**: Font weights and text sizing can be optimized
4. **Layout**: Card heights and arrangements can be tweaked
5. **Symbol Selection**: Unicode symbols vs ASCII fallbacks can be tested

## Next Steps

After establishing the visual design in this static implementation, the next milestone (T3.5) will integrate this styling into the interactive ah-tui crate with:

- Keyboard navigation between cards
- REST API data loading
- Real-time updates and state management
- Interactive task creation workflow

## Compliance with TUI-PRD.md

✅ **Header**: Agent Harbor branding with image support ready
✅ **Tasks**: Chronological list with proper card layouts for each state
✅ **Footer**: Context-specific shortcuts display
✅ **Visual Design**: Ratatui styling with proper colors and spacing
✅ **No Interactivity**: Pure static rendering for style development
✅ **Terminal Compatibility**: Works across different terminal sizes
