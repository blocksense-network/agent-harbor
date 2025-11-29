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

| Semantic Role                | Color Name     | Hex Code  | Description                                     |
| :--------------------------- | :------------- | :-------- | :---------------------------------------------- |
| **Base**                     | Base           | `#11111B` | Main application background                     |
| **Surface**                  | Mantle/Surface | `#242437` | Card and panel backgrounds                      |
| **Text**                     | Text           | `#CDD6F4` | Primary content text                            |
| **Muted**                    | Overlay0       | `#7F849C` | Secondary text, dim borders, metadata           |
| **Primary**                  | Blue           | `#89B4FA` | Primary actions, focused states, tool execution |
| **Accent**                   | Green          | `#A6E3A1` | Success states, file reads, active highlights   |
| **Warning**                  | Peach          | `#FAB387` | Warnings, operators, non-critical issues        |
| **Error**                    | Red            | `#F38BA8` | Errors, failures, deletions                     |
| **Border**                   | Surface1       | `#45475A` | Standard borders                                |
| **Border Focused**           | Blue           | `#89B4FA` | Active/Focused element borders                  |
| **Syntax:Keyword**           | Mauve          | `#CBA6F7` | Keywords (e.g., `fn`, `let`, `if`)              |
| **Syntax:String**            | Green          | `#A6E3A1` | String literals                                 |
| **Syntax:Function**          | Blue           | `#89B4FA` | Function definitions and calls                  |
| **Syntax:Type**              | Yellow         | `#F9E2AF` | Type names, classes, structs                    |
| **Syntax:Variable**          | Text           | `#CDD6F4` | Variable names                                  |
| **Syntax:Constant**          | Peach          | `#FAB387` | Constants, numeric literals                     |
| **Syntax:Comment**           | Overlay0       | `#7F849C` | Comments                                        |
| **CodeBg**                   | Surface0       | `#313244` | Background for code blocks                      |
| **TooltipBg**                | Surface2       | `#585B70` | Tooltip background color                        |
| **DimBorder**                | Surface0       | `#313244` | Dimmed border for discarded/future events       |
| **DimText**                  | Overlay1       | `#45475A` | Dimmed text for future events/history           |
| **DimError**                 | Surface2       | `#585B70` | Dimmed error color (e.g. idle stop button)      |
| **CodeHeaderBg**             | Surface1       | `#45475A` | Background for code block headers               |
| **CommandBg**                | Surface0       | `#313244` | Background for command lines                    |
| **OutputBg**                 | Base           | `#11111B` | Background for command output                   |
| **Gutter:Stderr:Background** | Red            | `#F38BA8` | Background for stderr gutter indicator          |
| **Gutter:Stderr:Foreground** | Base           | `#11111B` | Foreground for stderr gutter indicator          |
| **Terminal:Stdout**          | Text           | `#CDD6F4` | Standard terminal output                        |
| **Terminal:Stderr**          | Red            | `#F38BA8` | Standard error output                           |
| **Terminal:Command**         | Blue           | `#89B4FA` | Executed command text                           |
| **Terminal:Success**         | Green          | `#A6E3A1` | Success messages                                |
| **Terminal:Failure**         | Red            | `#F38BA8` | Failure messages                                |
| **Terminal:Warning**         | Peach          | `#FAB387` | Warning messages                                |
| **Terminal:Info**            | Blue           | `#89B4FA` | Info messages                                   |

## Semantic Usage Guidelines

### Status Indicators

- **Success**: `Green (Accent)` - Used for successful exit codes, file additions, and completed tasks.
- **Failure**: `Red (Error)` - Used for non-zero exit codes, file deletions, and error messages.
- **Warning**: `Peach (Warning)` - Used for pipeline operators (`|`, `&&`) and potential issues.
- **Running/Active**: `Blue (Primary)` - Used for active spinners, focused borders, and tool execution.

### Text Hierarchy

- **Primary Content**: `Text` - Standard output, main descriptions.
- **Secondary/Meta**: `Muted` - Timestamps, file paths, size indicators, dimmed history.
- **Headers**: `Blue (Primary)` or `Green (Accent)` depending on context (Action vs View).

### Syntax Highlighting (TUI Specific)

- **Commands**: `color:primary`
- **Arguments**: `color:text`
- **Flags**: `color:accent`
- **Operators**: `color:warning`

### Standard Syntax Highlighting

- **Keyword**: `color:syntax:keyword`
- **String**: `color:syntax:string`
- **Function**: `color:syntax:function`
- **Type**: `color:syntax:type`
- **Variable**: `color:syntax:variable`
- **Constant**: `color:syntax:constant`
- **Comment**: `color:syntax:comment`

### Terminal Output

- **Stdout**: `color:terminal:stdout`
- **Stderr**: `color:terminal:stderr`
- **Command**: `color:terminal:command`
- **Success**: `color:terminal:success`
- **Failure**: `color:terminal:failure`
- **Warning**: `color:terminal:warning`
- **Info**: `color:terminal:info`

### Specific Functional Roles

- **Keyboard Shortcuts**:
  - **Key**: `color:primary`
  - **Action Name**: `color:muted`
- **File Operations**:
  - **Added/Modified**: `color:accent`
  - **Deleted**: `color:error`
  - **Read**: `color:accent`
- **Command Elements**:
  - **Flag**: `color:accent`
  - **Operator**: `color:warning`
  - **Confirm Action**: `color:accent`
- **UI Elements**:
  - **Tooltip Text**: `color:tooltip`
  - **Tooltip Background**: `color:tooltip-bg`
  - **Stderr Gutter Background**: `color:gutter:stderr:background`
  - **Stderr Gutter Foreground**: `color:gutter:stderr:foreground`
