# TUI Color Theme Specification

This document defines the shared color theme and semantic color mappings used across all Agent Harbor TUI components (Dashboard, Agent Activity, SessionViewer).

## Notation

In design specifications, colors are referenced using the semantic notation **`color:semantic`**.

- **Semantic**: The role or meaning (e.g., Primary, Success, Error).

Example: `color:primary`, `color:success`, `color:error`.

The mapping from Semantic roles to actual colors is defined in the [Color Palette](#color-palette-catppuccin-mocha) section.

## Custom Color Themes

Color themes can be distributed as TOML files. Users can specify their preferred theme through the `tui-theme` configuration setting.

Example configuration:

```toml
[tui]
tui-theme = "path/to/my-theme.toml"
```

The TOML file should define the RGB values for each semantic role.

## Color Palette (Catppuccin Mocha)

The default theme is based on **Catppuccin Mocha**, optimized for dark terminal environments.

| Semantic Role                | Hex Code  | Description                                     |
| :--------------------------- | :-------- | :---------------------------------------------- |
| **base**                     | `#14141E` | Main application background                     |
| **surface**                  | `#1E1E2D` | Card and panel backgrounds                      |
| **text**                     | `#CDD6F4` | Primary content text                            |
| **muted**                    | `#7F849C` | Secondary text, dim borders, metadata           |
| **primary**                  | `#89B4FA` | Primary actions, focused states, tool execution |
| **accent**                   | `#96BE96` | Success states, file reads, active highlights   |
| **warning**                  | `#FAB387` | Warnings, operators, non-critical issues        |
| **error**                    | `#E1696E` | Errors, failures, deletions                     |
| **border**                   | `#45475A` | Standard borders                                |
| **border_focused**           | `#89B4FA` | Active/Focused element borders                  |
| **syntax:keyword**           | `#CBA6F7` | Keywords (e.g., `fn`, `let`, `if`)              |
| **syntax:string**            | `#96BE96` | String literals                                 |
| **syntax:function**          | `#89B4FA` | Function definitions and calls                  |
| **syntax:type**              | `#F9E2AF` | Type names, classes, structs                    |
| **syntax:variable**          | `#CDD6F4` | Variable names                                  |
| **syntax:constant**          | `#FAB387` | Constants, numeric literals                     |
| **syntax:comment**           | `#7F849C` | Comments                                        |
| **code_bg**                  | `#191923` | Background for code blocks                      |
| **tooltip_bg**               | `#232332` | Tooltip background color                        |
| **dim_border**               | `#2D2F3C` | Dimmed border for discarded/future events       |
| **dim_text**                 | `#5A5F6E` | Dimmed text for future events/history           |
| **dim_error**                | `#6E3C4B` | Dimmed error color (e.g. idle stop button)      |
| **code_header_bg**           | `#232332` | Background for code block headers               |
| **command_bg**               | `#1E1E2E` | Background for command lines                    |
| **output_bg**                | `#14141E` | Background for command output                   |
| **gutter:stderr:background** | `#E1696E` | Background for stderr gutter indicator          |
| **gutter:stderr:foreground** | `#14141E` | Foreground for stderr gutter indicator          |
| **terminal:stdout**          | `#CDD6F4` | Standard terminal output                        |
| **terminal:stderr**          | `#E1696E` | Standard error output                           |
| **terminal:command**         | `#89B4FA` | Executed command text                           |
| **terminal:success**         | `#96BE96` | Success messages                                |
| **terminal:failure**         | `#E1696E` | Failure messages                                |
| **terminal:warning**         | `#FAB387` | Warning messages                                |
| **terminal:info**            | `#89B4FA` | Info messages                                   |

## Semantic Usage Guidelines

### Status Indicators

- **Success**: `green (accent)` - Used for successful exit codes, file additions, and completed tasks.
- **Failure**: `red (error)` - Used for non-zero exit codes, file deletions, and error messages.
- **Warning**: `peach (warning)` - Used for pipeline operators (`|`, `&&`) and potential issues.
- **Running/Active**: `blue (primary)` - Used for active spinners, focused borders, and tool execution.

### Text Hierarchy

- **Primary Content**: `text` - Standard output, main descriptions.
- **Secondary/Meta**: `muted` - Timestamps, file paths, size indicators, dimmed history.
- **Headers**: `blue (primary)` or `green (accent)` depending on context (Action vs View).

## Spinners

Spinners are animated indicators used to show active processes or pending states. They are defined by a named sequence of characters (frames), a color, and a timing interval.

### Structure

Each spinner definition consists of:

- **Name**: The unique identifier used by the application (e.g., `awaiting_confirmation`).
- **Frames**: A sequence of frames to be drawn cyclically. Each frame consists of a text string and an optional specific duration.
  - **Inline Colors**: The text string supports inline semantic color tags in the format `{role}` (e.g., `{error}X{muted}Y`). These tags map to the corresponding theme colors.
- **Interval**: The default duration (in milliseconds) for frames that do not specify their own duration.
- **Color**: The default semantic color role used for the spinner if no inline color is specified. Must be a valid [Semantic Role](#color-palette-catppuccin-mocha).
- **Alt**: A static string displayed when animations are disabled or in non-interactive modes. This is defined by the application and supports i18n.

### Theme Overrides

Themes can override the **Frames**, **Interval**, and **Color** of a spinner. The **Name** and **Alt** text are fixed by the application logic to ensure consistent meaning.

### Standard Spinners

The following spinners are defined by the Agent Harbor specification:

#### `awaiting_confirmation`

Used when a user action (like sending a message) has been submitted locally but not yet acknowledged by the server.

- **Default Frames**: `⠋`, `⠙`, `⠹`, `⠸`, `⠼`, `⠴`, `⠦`, `⠧`, `⠇`, `⠏` (Standard Braille dots)
- **Default Interval**: 80ms
- **Default Color**: `color:muted`
- **Alt Text**: `...`

### Theme Configuration

In a theme TOML file, spinners are defined in a `[spinners]` table. You can define reusable frame sequences to easily assign the same animation to multiple spinners.

```toml
[spinners.sequences]
# Simple string sequence (uses spinner's default color and interval)
dots = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]

# Complex sequence with inline colors and durations (keyframes)
pulse = [
    { text = "{dim_text}•", duration = 100 },
    { text = "{text}•", duration = 300 },    # Hold bright state longer
    { text = "{dim_text}•", duration = 100 }
]

# Sequence using multi-character strings and mixed colors
multicolor = [
    { text = "{error}x{muted}y", duration = 200 },
    { text = "{muted}x{error}y", duration = 200 }
]

[spinners.definitions]
# Override the standard awaiting_confirmation spinner
awaiting_confirmation = { sequence = "dots", interval = 80, color = "muted" }

# Define another spinner using the pulse sequence
processing = { sequence = "pulse", interval = 100, color = "primary" }
```
