#!/usr/bin/env python3
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

import shutil
import textwrap
import re

# Catppuccin Mocha Colors (Modified for Desaturation)
COLORS = {
    "bg": (20, 20, 30), # Controlled App Background
    "surface": (30, 30, 45), # Slightly lighter for contrast if needed
    "text": (205, 214, 244),
    "muted": (127, 132, 156), # Overlay0/Subtext
    "dim_text": (90, 95, 110), # Dimmer than muted
    "primary": (137, 180, 250), # Blue
    "accent": (150, 190, 150), # Desaturated Green
    "warning": (250, 179, 135), # Peach
    "error": (225, 105, 110), # Redder (less orange), desaturated
    "dim_error": (110, 60, 75), # Even Dimmer Red for idle stop button
    "border": (69, 71, 90), # Surface1
    "dim_border": (45, 47, 60), # Even dimmer border for discarded cards
    "command_bg": (30, 30, 46), # Surface2
    "output_bg": (20, 20, 30), # Same as bg
    "code_bg": (25, 25, 35), # Slightly lighter than bg for code blocks
    "code_header_bg": (35, 35, 50), # Header for code blocks
}

# Semantic Color Constants
C_STDOUT = "text"
C_STDERR = "error"
C_META = "dim_text"

FILE_ICONS = {
    "rs": "",
    "py": "",
    "js": "",
    "ts": "",
    "html": "",
    "css": "",
    "md": "",
    "json": "",
    "txt": "",
    "log": "",
    "lock": "",
    "toml": "⚙",
    "conf": "⚙",
    "sh": "",
    "yml": "",
    "yaml": "",
}

MARGIN_X = 2
SHOW_TIMELINE = False
SHOW_STOP_BUTTON = False
USE_NERD_FONTS = False
APP_BG_KEY = "bg"
FOOTER_ALIGNMENT = "left" # "left", "center", "right"
FOOTER_MARGIN_X = 3

def color(text, fg_key=None, bg_key=None, bold=False):
    import os
    if os.environ.get("NO_COLOR"):
        return text

    code = ""
    if fg_key:
        r, g, b = COLORS[fg_key]
        code += f"\033[38;2;{r};{g};{b}m"
    
    # If bg_key is explicitly provided, use it.
    # Otherwise, we rely on print_line to set the global background.
    # However, to be safe for intra-line coloring:
    if bg_key:
        r, g, b = COLORS[bg_key]
        code += f"\033[48;2;{r};{g};{b}m"
    
    if bold:
        code += "\033[1m"
    
    reset = "\033[0m"
    return f"{code}{text}{reset}"

def get_file_icon(filename):
    if not USE_NERD_FONTS:
        return ""
    ext = filename.split(".")[-1] if "." in filename else ""
    return FILE_ICONS.get(ext, "")

def get_visual_len(text):
    import re
    ansi_escape = re.compile(r'\x1B(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~])')
    clean_text = ansi_escape.sub('', text)
    length = len(clean_text)
    for char in clean_text:
        # Simple heuristic for the emojis we use
        if char in "🧠🔧📝■📋📁🌿🤖🚀":
            length += 1
    return length

def print_line(text=""):
    import os
    no_color = os.environ.get("NO_COLOR")

    cols, _ = shutil.get_terminal_size()
    vlen = get_visual_len(text)
    padding = cols - vlen
    if padding < 0: padding = 0
    
    if no_color:
        print(text + " " * padding)
        return

    # Global BG color
    br, bg, bb = COLORS[APP_BG_KEY]
    bg_code = f"\033[48;2;{br};{bg};{bb}m"
    
    # Replace resets to ensure background persists
    # We replace \033[0m with \033[0m{bg_code}
    safe_text = text.replace("\033[0m", "\033[0m" + bg_code)
    
    # Print with global bg start, content, padding, and final reset
    print(f"{bg_code}{safe_text}{' ' * padding}\033[0m")

def print_centered(text, width, style_args={}):
    padding = (width - len(text)) // 2
    # Construct the line content with margins
    # Note: width here is content_width (e.g. 80), not terminal width
    # We need to respect MARGIN_X
    
    line_content = " " * MARGIN_X + " " * padding + color(text, **style_args)
    # print_line will handle the rest of the width (right margin)
    print_line(line_content)

def highlight_code(line, lang="text"):
    # Expand tabs to 2 spaces
    line = line.replace("\t", "  ")
    
    import re
    # Split by whitespace, keeping delimiters to preserve exact spacing
    parts = re.split(r'(\s+)', line)
    colored_parts = []
    
    for part in parts:
        if not part: continue
        
        if part.isspace():
            colored_parts.append(part)
            continue
            
        # Keywords
        if part in ["def", "class", "return", "import", "from", "if", "else", "for", "while", "fn", "let", "mut", "pub", "impl"]:
            colored_parts.append(color(part, "primary", bold=True))
        # Strings (heuristic: starts with quote)
        elif part.startswith('"') or part.startswith("'"):
            colored_parts.append(color(part, "accent"))
        # Comments
        elif part.startswith("#") or part.startswith("//"):
            colored_parts.append(color(part, "muted"))
        # Functions (heuristic)
        elif "(" in part:
            name = part.split("(")[0]
            rest = part[len(name):]
            colored_parts.append(color(name, "primary") + color(rest, "text"))
        else:
            colored_parts.append(color(part, "text"))
            
    return "".join(colored_parts)

def highlight_command_syntax(cmd_str):
    parts = cmd_str.split(" ")
    colored_parts = []
    
    expect_command = True
    
    for part in parts:
        if not part: # Handle multiple spaces
            colored_parts.append(" ")
            continue
            
        if part in ["|", "&&", ";", ">", ">>"]:
            colored_parts.append(color(part, "warning", bold=True))
            expect_command = True
        elif expect_command:
            colored_parts.append(color(part, "primary", bold=True))
            expect_command = False
        elif part.startswith("-"):
            colored_parts.append(color(part, "accent"))
        else:
            colored_parts.append(color(part, "text"))
            
    return " ".join(colored_parts)

def draw_control_box(segments, c_border="border", content_lens=None):
    # segments is list of (text, color_key)
    # separator is "│" (colored border) - TIGHT
    
    inner_parts = []
    clean_inner_parts = []
    
    sep_colored = color("│", c_border)
    sep_clean = "│"
    
    for i, (txt, col) in enumerate(segments):
        inner_parts.append(color(txt, col))
        clean_inner_parts.append(txt)
        if i < len(segments) - 1:
            inner_parts.append(sep_colored)
            clean_inner_parts.append(sep_clean)
            
    inner_str = "".join(inner_parts)
    
    # Build Caps
    top_parts = []
    bot_parts = []
    
    content_idx = 0
    for i, txt in enumerate(clean_inner_parts):
        if txt == sep_clean:
            # Separator caps - TIGHT
            top_parts.append("┬")
            bot_parts.append("┴")
        else:
            # Content caps
            if content_lens and content_idx < len(content_lens):
                vlen = content_lens[content_idx]
                content_idx += 1
            else:
                vlen = get_visual_len(txt)
            top_parts.append("─" * vlen)
            bot_parts.append("─" * vlen)
            
    top_inner = "".join(top_parts)
    bot_inner = "".join(bot_parts)
    
    # Construct full strings
    # Top: ╭{top_inner}╮
    # Mid: ┤{inner_str}├
    # Bot: ╰{bot_inner}╯
    
    top_str = "╭" + top_inner + "╮"
    bot_str = "╰" + bot_inner + "╯"
    mid_str = color("┤", c_border) + inner_str + color("├", c_border)
    
    # Calculate total visual length of the box (for layout)
    # Length = 1 (┤) + inner_len + 1 (├)
    # inner_len should match top_inner length
    total_len = 1 + len(top_inner) + 1
    
    return top_str, mid_str, bot_str, total_len

def draw_hero_card(title, content, width):
    # Hero card uses standard border, but colored title
    c_border = "border"
    c_accent = "primary"
    
    # Title Decoration: ┤ Title ├
    clean_title = title.replace("🧠 ", "").replace("🔧 ", "").replace("📝 ", "").upper()
    
    # Construct Top Line with mixed colors
    # Line -1:   ╭─────╮       ╭─────╮
    # Line  0: ╭─┤ TITLE ├──────┤ ✥ ▼ ├─╮
    # Line +1: │ ╰─────╯       ╰─────╯ │
    
    # Parts
    start_marker = "╭─"
    title_block = "┤ " + clean_title + " ├"
    
    # Right Block: Fullscreen + Collapse (No timestamp for hero)
    # Segments: ✥, ▼
    # Added padding: " ✥ ", " ▼ "
    segments = [(" ❐ ", "text"), (" ▼ ", "text")]
    right_top, right_mid, right_bot, len_close_block = draw_control_box(segments, c_border)
    
    # Lengths
    len_start = get_visual_len(start_marker)
    len_title_block = get_visual_len(title_block)
    
    # Adjust len_close_block for the trailing "─╮" which is NOT part of the box itself
    # The box is "┤ ... ├". We need to add "─╮" after it.
    # Wait, draw_control_box returns the box string.
    # We need to append "─╮" to the mid line.
    # And we need to account for its length.
    
    # Total right block visual len = len_close_block + 2 (for "─╮")
    
    dash_len = width - len_start - len_title_block - (len_close_block + 2)
    if dash_len < 0: dash_len = 0
    
    # Top Cap for Title
    title_cap_str = "╭" + "─" * (len_title_block - 2) + "╮"
    
    # Render Line -1 (Caps)
    line_minus_1 = " " * MARGIN_X + " " * len_start + color(title_cap_str, c_border)
    pad_to_close = dash_len
    
    line_minus_1 += " " * pad_to_close + color(right_top, c_border)
    print_line(line_minus_1)
    
    # Render Line 0 (Main Top Border)
    top_line = (
        color(start_marker, c_border) + 
        color("┤ ", c_border) + color(clean_title, c_accent, bold=True) + color(" ├", c_border) + 
        color("─" * dash_len, c_border) + 
        right_mid + color("─╮", c_border)
    )
    print_line(" " * MARGIN_X + top_line)
    
    # Render Line +1 (Bottom Caps + Spacer)
    title_bot_cap = "╰" + "─" * (len_title_block - 2) + "╯"
    
    pad_mid = dash_len
    
    line_plus_1 = (
        " " * MARGIN_X + 
        color("│ ", c_border) + 
        color(title_bot_cap, c_border) + 
        " " * pad_mid + 
        color(right_bot, c_border) + 
        color(" │", c_border)
    )
    print_line(line_plus_1)
    
    # Content
    wrapper = textwrap.TextWrapper(width=width-4)
    wrapped_lines = wrapper.wrap(content)
    
    for line in wrapped_lines:
        padding = width - 4 - get_visual_len(line)
        print_line(" " * MARGIN_X + color("│ ", c_border) + color(line, "text", bold=True) + " " * padding + color(" │", c_border))
        
    # Bottom Border
    print_line(" " * MARGIN_X + color("╰" + "─" * (width - 2) + "╯", c_border))

def draw_timeline_event(timestamp, icon, title, content_lines, width, color_key="border", command=None, diff_summary=None, output_size=None, pipeline_statuses=None, is_markdown=False, dimmed=False):
    if SHOW_TIMELINE:
        timeline_indent = 2
        card_start_indent = 4 
        connector = "├─ "
    else:
        timeline_indent = 0
        card_start_indent = 0
        connector = ""
        
    card_w = width - timeline_indent - card_start_indent
    
    # Dimming Logic
    if dimmed:
        c_border = "dim_border" # Even dimmer border
        c_title = "dim_text"
        c_text = "dim_text"
        c_icon = "dim_text"
        c_meta = "dim_text"
        c_accent = "dim_text"
        c_error = "dim_text"
    else:
        c_border = "border"
        c_title = color_key
        c_text = "text"
        c_icon = "text" # Icons usually text color or specific
        c_meta = C_META
        c_accent = "accent"
        c_error = "error"

    # Clean and Format Title
    clean_title = title.replace("🧠 ", "").replace("🔧 ", "").replace("📝 ", "").replace("🗑️ ", "").replace("✅ ", "")
    
    # Parse Category and Name
    if ":" in clean_title:
        parts = clean_title.split(":", 1)
        category = parts[0].strip().upper()
        name = parts[1].strip()
    else:
        category = clean_title.upper()
        name = ""

    # Rename Categories
    if category == "TOOLS" or category == "TOOL": category = "RAN"
    if category == "FILES" or category == "FILE": category = "READ"
    if category == "EDITS" or category == "EDIT": category = "EDITED"
    if category == "TASK": category = "COMPLETED"
        
    # Construct Title Block Content (Left Side)
    
    # Stop button:  ■ (Dimmed Red square)
    stop_btn = color(" ■", "dim_error" if not dimmed else "dim_text")
    stop_btn_len = 2 
    
    if category == "RAN":
        # Handle pipelines
        import re
        # Split by | or &&, capturing the separator
        parts = re.split(r'(\s*(?:\||&&)\s*)', name)
        
        if len(parts) > 1:
            cmds = parts[0::2]
            seps = parts[1::2]
            
            display_parts = []
            visual_len_acc = 0
            
            # Category
            display_parts.append(color(category + " ", c_title, bold=not dimmed))
            visual_len_acc += len(category) + 1
            
            # Determine which command gets the output size
            output_sizes_map = {}
            if isinstance(output_size, list):
                for idx, size in enumerate(output_size):
                    if size:
                        output_sizes_map[idx] = size
            elif output_size:
                # Legacy behavior: attach to last executed
                target_idx = len(cmds) - 1
                if pipeline_statuses:
                    for idx in range(len(cmds) - 1, -1, -1):
                        if idx < len(pipeline_statuses) and pipeline_statuses[idx] is not None:
                            target_idx = idx
                            break
                output_sizes_map[target_idx] = output_size
            
            for i, cmd in enumerate(cmds):
                # Only show command name in title, not args
                cmd_name = cmd.strip().split(" ")[0]
                
                # Determine color for this command
                if dimmed:
                    cmd_color = "dim_text"
                else:
                    cmd_color = c_title
                    if pipeline_statuses and i < len(pipeline_statuses):
                        status = pipeline_statuses[i]
                        if status == 0:
                            cmd_color = "accent"
                        elif status == 1:
                            cmd_color = "error"
                        else:
                            cmd_color = "muted"
                
                display_parts.append(color(cmd_name, cmd_color))
                visual_len_acc += len(cmd_name)
                
                # Output Size
                if i in output_sizes_map:
                    size_str = " " + output_sizes_map[i]
                    display_parts.append(color(size_str, c_meta))
                    visual_len_acc += len(size_str)
                
                # Add stop button if enabled
                if SHOW_STOP_BUTTON:
                    display_parts.append(stop_btn)
                    visual_len_acc += stop_btn_len
                
                if i < len(cmds) - 1:
                    # Separator
                    raw_sep = seps[i]
                    if "&&" in raw_sep:
                        sep_str = " && "
                    else:
                        sep_str = " | "
                    
                    display_parts.append(color(sep_str, "muted"))
                    visual_len_acc += len(sep_str)
            
            title_inner_colored = "".join(display_parts)
            title_inner_len = visual_len_acc
            
        else:
            # Single command
            cmd_name = name.split(" ")[0]
            
            size_part = ""
            size_len = 0
            if output_size:
                size_part = color(" " + output_size, c_meta)
                size_len = 1 + len(output_size)
            
            stop_part = ""
            stop_len = 0
            if SHOW_STOP_BUTTON:
                stop_part = stop_btn
                stop_len = stop_btn_len
            
            title_inner_colored = color(category + " ", c_title, bold=not dimmed) + color(cmd_name, c_title) + size_part + stop_part
            title_inner_len = len(category) + 1 + len(cmd_name) + size_len + stop_len
            
    elif category == "EDITED":
         # Add diff summary if present: +X -Y
         summary_str = ""
         summary_len = 0
         if diff_summary:
             added, removed = diff_summary
             if dimmed:
                 summary_str = color(f" +{added} -{removed}", "dim_text")
             else:
                 summary_str = color(f" +{added}", "accent") + color(f" -{removed}", "error")
             summary_len = len(f" +{added} -{removed}")
             
         # Handle icon separation for coloring
         # Heuristic: if name contains space and first char is icon
         # With Nerd Fonts disabled, name might just be filename
         
         parts = name.split(" ", 1)
         if len(parts) == 2 and ord(parts[0][0]) > 127:
             icon = parts[0]
             fname = parts[1]
             name_colored = color(icon, c_text) + " " + color(fname, c_text) # Neutral filename
         else:
             name_colored = color(name, c_text) # Neutral filename
             
         title_inner_colored = color(category + " ", c_title, bold=not dimmed) + name_colored + summary_str
         title_inner_len = len(category) + 1 + len(name) + summary_len
         
    elif category == "READ":
         # READ category - Now Green (Accent)
         title_inner_colored = color(category, c_title, bold=not dimmed)
         title_inner_len = len(category)
         
    elif category == "DELETED":
         # DELETED category - Just title, content is in body
         title_inner_colored = color(category, c_title, bold=not dimmed) # Use passed color (accent/green)
         title_inner_len = len(category)

    else:
        # THOUGHT, COMPLETED or others
        title_inner_colored = color(category, c_title, bold=not dimmed)
        title_inner_len = len(category)
        
    
    # Parts
    start_marker = "╭─"
    title_block_colored = color("┤ ", c_border) + title_inner_colored + color(" ├", c_border)
    len_title_block = 2 + title_inner_len + 2
    
    # Right Block: Fullscreen + Collapse + Timestamp
    # Segments: ✥, ▼, timestamp
    # Added padding: " ✥ ", " ▼ ", " " + timestamp + " "
    segments = [(" ❐ ", c_text), (" ▼ ", c_text), (" " + timestamp + " ", "muted")]
    right_top, right_mid, right_bot, len_close_block = draw_control_box(segments, c_border)
    
    len_start = get_visual_len(start_marker)
    
    dash_len = card_w - len_start - len_title_block - (len_close_block + 2)
    if dash_len < 0: dash_len = 0
    
    # Line -1 (Caps)
    title_cap_str = "╭" + "─" * (len_title_block - 2) + "╮"
    
    line_minus_1 = " " * MARGIN_X + " " * timeline_indent
    if SHOW_TIMELINE:
        line_minus_1 += color("│", "muted") + " " * (card_start_indent - 1)
    
    line_minus_1 += " " * len_start + color(title_cap_str, c_border)
    
    pad_to_close = dash_len
    
    line_minus_1 += " " * pad_to_close + color(right_top, c_border)
    print_line(line_minus_1)
    
    # Line 0 (Top Border)
    card_top = (
        color(start_marker, c_border) + 
        title_block_colored + 
        color("─" * dash_len, c_border) + 
        right_mid + color("─╮", c_border)
    )
    
    prefix = " " * MARGIN_X + " " * timeline_indent
    if SHOW_TIMELINE:
        prefix += color(connector, "muted")
        
    print_line(prefix + card_top)
    
    # Line +1 (Bottom Caps + Spacer)
    title_bot_cap = "╰" + "─" * (len_title_block - 2) + "╯"
    
    pad_mid = dash_len
    
    line_plus_1 = " " * MARGIN_X + " " * timeline_indent
    if SHOW_TIMELINE:
        line_plus_1 += color("│  ", "muted")
        
    line_plus_1 += (
        color("│ ", c_border) + 
        color(title_bot_cap, c_border) + 
        " " * pad_mid + 
        color(right_bot, c_border) + 
        color(" │", c_border)
    )
    print_line(line_plus_1)
    
    # Card Content
    # If command is present, print it first with syntax highlighting and no background
    if command:
        visible_len = get_visual_len(command)
        padding = card_w - 4 - visible_len - 2 # -2 for "$ "
        if padding < 0: padding = 0
        
        prefix = " " * MARGIN_X + " " * timeline_indent
        if SHOW_TIMELINE:
            prefix += color("│  ", "muted")
            
        # Command line: $ command
        # No background color
        
        if dimmed:
            highlighted_cmd = color(command, "dim_text")
        else:
            highlighted_cmd = highlight_command_syntax(command)
            
        inner_content = color("$ ", "dim_text" if dimmed else "muted") + highlighted_cmd + " " * padding
        print_line(prefix + color("│ ", c_border) + inner_content + color(" │", c_border))

    # Content Rendering (Markdown or Plain)
    if is_markdown:
        in_code_block = False
        code_lang = ""
        
        for line in content_lines:
            prefix = " " * MARGIN_X + " " * timeline_indent
            if SHOW_TIMELINE:
                prefix += color("│  ", "muted")
            
            if line.strip().startswith("```"):
                if not in_code_block:
                    # Start Code Block
                    in_code_block = True
                    code_lang = line.strip().replace("```", "")
                    
                    # External Empty Line (Normal BG) before block
                    inner_w = card_w - 4
                    empty_line_normal = " " * inner_w
                    print_line(prefix + color("│ ", c_border) + color(empty_line_normal, None, "output_bg") + color(" │", c_border))
                    
                    # Draw Header: [ Language           ❐ ]
                    # Header BG: code_header_bg
                    
                    header_content = " " + code_lang
                    copy_icon = "❐ "
                    
                    # Calculate padding
                    pad_len = inner_w - len(header_content) - len(copy_icon)
                    if pad_len < 0: pad_len = 0
                    
                    if dimmed:
                        header_str = (
                            color(header_content, "dim_text", "output_bg") + 
                            color(" " * pad_len, None, "output_bg") + 
                            color(copy_icon, "dim_text", "output_bg")
                        )
                    else:
                        header_str = (
                            color(header_content, "text", "code_header_bg", bold=True) + 
                            color(" " * pad_len, None, "code_header_bg") + 
                            color(copy_icon, "primary", "code_header_bg")
                        )
                    
                    print_line(prefix + color("│ ", c_border) + header_str + color(" │", c_border))
                    
                    # Internal Empty Line (Code BG) - RESTORED
                    bg_code = ""
                    if not dimmed:
                        r, g, b = COLORS["code_bg"]
                        bg_code = f"\033[48;2;{r};{g};{b}m"
                    
                    empty_line_code = bg_code + " " * inner_w + "\033[0m"
                    print_line(prefix + color("│ ", c_border) + empty_line_code + color(" │", c_border))
                    
                else:
                    # End Code Block
                    
                    # Internal Empty Line (Code BG) - RESTORED
                    bg_code = ""
                    if not dimmed:
                        r, g, b = COLORS["code_bg"]
                        bg_code = f"\033[48;2;{r};{g};{b}m"
                        
                    inner_w = card_w - 4
                    empty_line_code = bg_code + " " * inner_w + "\033[0m"
                    print_line(prefix + color("│ ", c_border) + empty_line_code + color(" │", c_border))
                    
                    in_code_block = False
                    code_lang = ""
                    
                    # External Empty Line (Normal BG) after block
                    inner_w = card_w - 4
                    empty_line_normal = " " * inner_w
                    print_line(prefix + color("│ ", c_border) + color(empty_line_normal, None, "output_bg") + color(" │", c_border))
                    
            else:
                if in_code_block:
                    # Render Code Line
                    # BG: code_bg
                    # Syntax Highlight
                    
                    if dimmed:
                        highlighted = color(line.replace("\t", "  "), "dim_text")
                    else:
                        highlighted = highlight_code(line, code_lang)
                    
                    # Add 1 space padding left/right
                    # Content: " " + highlighted + " "
                    # We need to calculate padding to fill the rest of the line
                    
                    # Visual len of the code content (including our 1 space left)
                    # Note: highlight_code handles tabs -> spaces
                    # Use simple length of expanded line to be safe
                    code_vlen = len(line.replace("\t", "  "))
                    
                    # Total content vlen = 1 (left space) + code_vlen
                    # We want to fill up to inner_w
                    
                    inner_w = card_w - 4
                    
                    # Construct the line with code_bg applied everywhere
                    bg_code = ""
                    if not dimmed:
                        r, g, b = COLORS["code_bg"]
                        bg_code = f"\033[48;2;{r};{g};{b}m"
                    
                    # Safe Highlight: replace resets with reset+bg
                    safe_highlighted = highlighted.replace("\033[0m", "\033[0m" + bg_code)
                    
                    # Calculate right padding
                    # We have 1 space left, code, then we need right padding
                    # Total visual length so far: 1 + code_vlen
                    right_pad_len = inner_w - (1 + code_vlen)
                    if right_pad_len < 0: right_pad_len = 0
                    
                    content_str = bg_code + " " + safe_highlighted + " " * right_pad_len + "\033[0m"
                    
                    print_line(prefix + color("│ ", c_border) + content_str + color(" │", c_border))
                    
                else:
                    # Normal Markdown Text
                    # Handle bullets
                    display_line = line.replace("- ", "• ")
                    
                    # Handle Headers
                    style_args = {"fg_key": c_text, "bg_key": "output_bg"}
                    if display_line.startswith("# "):
                        display_line = display_line.replace("# ", "")
                        if dimmed:
                            style_args = {"fg_key": "dim_text", "bg_key": "output_bg", "bold": False}
                        else:
                            style_args = {"fg_key": "primary", "bg_key": "output_bg", "bold": True}
                    
                    vlen = get_visual_len(display_line)
                    padding = card_w - 4 - vlen
                    if padding < 0: padding = 0
                    
                    # Apply output_bg (which is same as app bg)
                    inner_content = color(display_line, **style_args) + color(" " * padding, None, "output_bg")
                    
                    print_line(prefix + color("│ ", c_border) + inner_content + color(" │", c_border))

    else:
        # Standard Output lines
        for line in content_lines:
            visible_len = get_visual_len(line)
            padding = card_w - 4 - visible_len
            if padding < 0: padding = 0
            
            prefix = " " * MARGIN_X + " " * timeline_indent
            if SHOW_TIMELINE:
                prefix += color("│  ", "muted")
                
            # Apply output_bg
            inner_content = color(line, None, "output_bg") + color(" " * padding, None, "output_bg")
            
            print_line(prefix + color("│ ", c_border) + inner_content + color(" │", c_border))
        
    # Card Bottom
    prefix = " " * MARGIN_X + " " * timeline_indent
    if SHOW_TIMELINE:
        prefix += color("│  ", "muted")
        
    print_line(prefix + color("╰" + "─" * (card_w - 2) + "╯", c_border))
    
    if SHOW_TIMELINE:
        print_line(" " * MARGIN_X + " " * timeline_indent + color("│", "muted"))

def draw_footer():
    cols, _ = shutil.get_terminal_size()
    
    # Footer content: Shortcuts with long names and centered style
    # Ctrl+C Stop, Ctrl+D Detach, Ctrl+L Clear, Shift+Enter New Line
    
    shortcuts = [
        ("Ctrl+C", "Stop"),
        ("Ctrl+D", "Detach"),
        ("Ctrl+L", "Clear"),
        ("Shift+Enter", "New Line")
    ]
    
    parts = []
    for key, label in shortcuts:
        # Key in Primary, Label in Muted (Restored style)
        parts.append(color(key, "primary", bold=True) + " " + color(label, "muted"))
        
    # Separator: 3 spaces (as per original centered style) or bullet?
    # User said "restore them" (colors) but "use long descriptive names".
    # Previous centered style used 3 spaces.
    # Dashboard style used bullet.
    # I'll use 3 spaces to be safe with "restore them", or maybe bullet?
    # "I actually liked the colors that you used for the footer in your previous iteration"
    # Previous iteration (Step 674) used 3 spaces.
    # I will use 3 spaces.
    
    content = "   ".join(parts)
    vlen = get_visual_len(content)
    
    # Calculate padding based on alignment
    if FOOTER_ALIGNMENT == "center":
        left_pad = (cols - vlen) // 2
    elif FOOTER_ALIGNMENT == "right":
        left_pad = cols - vlen - FOOTER_MARGIN_X
    else: # left
        left_pad = FOOTER_MARGIN_X
        
    if left_pad < 0: left_pad = 0
    right_pad = cols - vlen - left_pad
    if right_pad < 0: right_pad = 0
    
    # Print footer line with surface background
    bg_key = "surface"
    r, g, b = COLORS[bg_key]
    bg_code = f"\033[48;2;{r};{g};{b}m"
    
    line = bg_code + " " * left_pad + content + " " * right_pad + "\033[0m"
    print(line)

def draw_instructions_card(width, active=True, show_title=False):
    if active:
        c_border = "primary"
        c_title = "primary"
        c_btn_go_bg = None
        c_btn_go_fg = "accent"
        c_btn_opts = "primary"
        placeholder_color = "muted"
    else:
        c_border = "border" # Dimmed
        c_title = "border"
        c_btn_go_bg = None
        c_btn_go_fg = "muted"
        c_btn_opts = "border"
        placeholder_color = "dim_text"
    
    # Title: INSTRUCTIONS
    title = "INSTRUCTIONS"
    
    if show_title:
        # Title Box Construction
        title_inner = color(" " + title + " ", c_title, bold=active)
        title_box = color("┤", c_border) + title_inner + color("├", c_border)
        
        len_title_inner = get_visual_len(" " + title + " ")
        len_title_box = 1 + len_title_inner + 1
        
        # Top Caps for Title (Line -1)
        title_top_cap = "╭" + "─" * len_title_inner + "╮"
        
        # Bottom Caps for Title (Line +1)
        title_bot_cap = "╰" + "─" * len_title_inner + "╯"
    else:
        # No Title
        title_box = ""
        len_title_box = 0
        len_title_inner = 0
        title_top_cap = ""
        title_bot_cap = ""
    
    # Top Border Line (Line 0)
    start_marker = "╭─"
    len_start = get_visual_len(start_marker)
    
    dash_len = width - len_start - len_title_box - 1 # -1 for closing ╮
    if dash_len < 0: dash_len = 0
    
    # Draw Line -1 (Outside)
    if show_title:
        print_line(" " * MARGIN_X + " " * len_start + color(title_top_cap, c_border))
    
    # Draw Line 0 (Border)
    if show_title:
        top_line = (
            color(start_marker, c_border) + 
            title_box + 
            color("─" * dash_len + "╮", c_border)
        )
    else:
        # Just a full line
        top_line = color(start_marker + "─" * (width - len_start - 1) + "╮", c_border)
        
    print_line(" " * MARGIN_X + top_line)
    
    # Draw Line +1 (Inside - First line of content area)
    # │ + TitleBotCap + Padding + │
    
    if show_title:
        # FIX: Padding calculation
        # Content width = width - 4 (2 for '│ ', 2 for ' │')
        # Object is title_bot_cap, len = len_title_inner + 2
        # Padding = Content_width - Object_len
        #         = (width - 4) - (len_title_inner + 2)
        #         = width - 6 - len_title_inner
        
        line_plus_1_padding = width - 6 - len_title_inner
        if line_plus_1_padding < 0: line_plus_1_padding = 0
        
        line_plus_1 = (
            color("│ ", c_border) + 
            color(title_bot_cap, c_border) + 
            " " * line_plus_1_padding + 
            color(" │", c_border)
        )
        print_line(" " * MARGIN_X + line_plus_1)
    
    # Content Area
    # Placeholder Text
    placeholder = "Describe your task..."
    len_ph = get_visual_len(placeholder)
    pad_ph = width - 4 - len_ph
    if pad_ph < 0: pad_ph = 0
    
    print_line(" " * MARGIN_X + color("│ ", c_border) + color(placeholder, placeholder_color) + " " * pad_ph + color(" │", c_border))
    # Empty line
    print_line(" " * MARGIN_X + color("│", c_border) + " " * (width - 2) + color("│", c_border))
        
    # Separator (Empty line)
    print_line(" " * MARGIN_X + color("│", c_border) + " " * (width - 2) + color("│", c_border))
    
    # Bottom Buttons Construction
    # Left: [ 🤖 MODELS ]
    # Right: [ ⏎ GO │ ≡ OPTIONS ]
    
    # Left Box
    seg_models = [(" 🤖 MODELS ", c_title)]
    l_top, l_mid, l_bot, l_len = draw_control_box(seg_models, c_border)
    
    # Right Box
    # Hack: Pass pre-colored string as text, and None as color_key
    
    go_btn_txt = " ⏎ GO "
    if active:
        go_colored = color(go_btn_txt, c_btn_go_fg, c_btn_go_bg, bold=True)
    else:
        go_colored = color(go_btn_txt, c_btn_go_fg, c_btn_go_bg) # Dimmed style
        
    opts_txt = " ≡ OPTIONS "
    opts_colored = color(opts_txt, c_btn_opts, bold=active)
    
    seg_right_custom = [(go_colored, None), (opts_colored, None)]
    # Manually specify lengths for GO and OPTIONS to handle single-width rendering of ⏎ and ≡
    # " ⏎ GO " -> 6 chars
    # " ≡ OPTIONS " -> 11 chars
    r_top, r_mid, r_bot, r_len = draw_control_box(seg_right_custom, c_border, content_lens=[6, 11])
    
    # Bottom Border Line Construction
    end_marker = "─╯"
    len_end = get_visual_len(end_marker)
    
    bot_start_marker = "╰─"
    len_bot_start = get_visual_len(bot_start_marker)
    
    dash_len_bot = width - len_bot_start - l_len - r_len - len_end
    if dash_len_bot < 0: dash_len_bot = 0
    
    # Line N-1 (Inside - Top Caps of Buttons)
    line_n_minus_1 = (
        color("│ ", c_border) + 
        color(l_top, c_border) + 
        " " * dash_len_bot + 
        color(r_top, c_border) + 
        color(" │", c_border)
    )
    print_line(" " * MARGIN_X + line_n_minus_1)
    
    # Draw Line N (Bottom Border with Buttons)
    bot_line = (
        color(bot_start_marker, c_border) + 
        l_mid + 
        color("─" * dash_len_bot, c_border) + 
        r_mid + 
        color(end_marker, c_border)
    )
    print_line(" " * MARGIN_X + bot_line)
    
    # Draw Line N+1 (Outside - Bottom Caps of Buttons)
    line_n_plus_1 = (
        " " * MARGIN_X + 
        " " * len_bot_start + 
        color(l_bot, c_border) + 
        " " * dash_len_bot + 
        color(r_bot, c_border)
    )
    print_line(line_n_plus_1)

def main():
    cols, rows = shutil.get_terminal_size()
    
    # Effective width for content
    content_width = min(80, cols - 2 * MARGIN_X) # Cap width at 80 chars for readability
    
    print_line("")
    
    # 1. Thought (Active) - Removed as per request
    # draw_hero_card("🧠 THOUGHT", ...
    
    # Timeline Events
    
    # 1. Thought
    draw_timeline_event(
        "14:22",
        "🧠",
        "Thought",
        [
            color("The error 'Not implemented' suggests I missed a case", "text"),
            color("in the handle_write function. Checking the source.", "text")
        ],
        content_width,
        color_key="accent"
    )

    # 2. RAN (Pipeline with error)
    draw_timeline_event(
        "14:22", "🔧", "RAN: cat | grep && sort", 
        [
            color("processing file.txt...", C_META),
            color("error: something went wrong", C_STDERR),
            color("error: another failure", C_STDERR)
        ],
        content_width,
        color_key="error",
        command="cat file.txt | grep error && sort",
        output_size=["12KB", "213B", None],
        pipeline_statuses=[0, 1, None] # 0=Success, 1=Error, None=Skipped
    )
    
    # 3. Complex Command Execution (Success)
    draw_timeline_event(
        "14:22",
        "🔧",
        "RAN: grep -r pattern src",
        [
            color("src/main.rs:10: pattern found", C_META),
            color("src/lib.rs:5: pattern found", C_META)
        ],
        content_width,
        "accent", # Green for success
        command="grep -r pattern src",
        output_size="12K"
    )
    
    # 4. File Read (FILES -> READ)
    # Add icons
    f1 = "src/interpose.rs"
    f2 = "src/main.rs"
    i1 = get_file_icon(f1)
    i2 = get_file_icon(f2)
    
    # Helper to construct icon+name string
    def fmt_file(i, f):
        if i: return color(i, "text") + " " + color(f, "muted")
        return color(f, "muted")
    
    draw_timeline_event(
        "14:22",
        "📄",
        "READ",
        [
            fmt_file(i1, f1) + color(" (lines 40-50)", "dim_text"),
            fmt_file(i2, f2) + color(" (lines 10-20)", "dim_text")
        ],
        content_width,
        "accent" # Green (Accent)
    )
    
    # 5. File Edit (EDITS -> EDITED)
    f3 = "src/main.rs"
    i3 = get_file_icon(f3)
    
    # Constructing a diff line with intra-line highlighting
    # Old: -    println!("Hello");
    # New: +    println!("Hello, World!");
    
    # We'll use manual ANSI construction for the highlighted parts
    # Base colors (foreground)
    c_err = COLORS["error"]
    c_acc = COLORS["accent"]
    c_text = COLORS["text"]
    
    # Background colors for diffs (dimmed versions of error/accent)
    # In a real TUI we'd calculate these or have them in the theme
    # For mockup, let's approximate:
    # Error BG: Dark Red (60, 20, 20)
    # Accent BG: Dark Green (20, 60, 20)
    bg_err = (60, 25, 35)
    bg_acc = (25, 50, 35)
    
    # Highlight BG colors (brighter/more opaque versions)
    # Error Highlight: (100, 40, 40)
    # Accent Highlight: (40, 80, 40)
    bg_err_hl = (100, 40, 55)
    bg_acc_hl = (40, 80, 55)
    
    def style_diff(text, type_key, highlight_ranges=[]):
        # type_key: "error" or "accent"
        if type_key == "error":
            fg = c_text # Keep text readable
            bg = bg_err
            bg_hl = bg_err_hl
        else:
            fg = c_text
            bg = bg_acc
            bg_hl = bg_acc_hl
            
        # Base ANSI for the line
        base_code = f"\033[38;2;{fg[0]};{fg[1]};{fg[2]}m\033[48;2;{bg[0]};{bg[1]};{bg[2]}m"
        reset = "\033[0m"
        
        if not highlight_ranges:
            # Pad the background to the end of the visual line? 
            # For simplicity in mockup, just color the text
            return f"{base_code}{text}{reset}"
            
        result = base_code
        last_idx = 0
        for start, end in highlight_ranges:
            result += text[last_idx:start]
            # Highlight segment with brighter background
            result += f"\033[48;2;{bg_hl[0]};{bg_hl[1]};{bg_hl[2]}m{text[start:end]}\033[48;2;{bg[0]};{bg[1]};{bg[2]}m"
            last_idx = end
        result += text[last_idx:] + reset
        return result

    # 5. Edit File
    draw_timeline_event(
        "14:22", "📝", "EDITED: src/main.rs", 
        [
            color("@@ -15,7 +15,10 @@", "muted"),
            color("     fn main() {", "text"),
            
            # -    println!("Hello");
            # Highlight "Hello" vs "Hello, World!"
            
            style_diff("-    println!(\"Hello\");", "error"),
            style_diff("+    println!(\"Hello, World!\");", "accent", [(19, 27)]), # Highlight ", World!"
            
            color("     }", "text")
        ],
        content_width,
        color_key="accent", # Green for success
        diff_summary=(1, 1)
    )

    # 6. Delete File
    draw_timeline_event(
        "14:23", "🗑️", "DELETED", 
        [
            color("temp_debug.log", "text")
        ],
        content_width,
        color_key="accent" # Green for success
    )
    
    # 7. Thought (Success)
    draw_timeline_event(
        "14:24", "🧠", "AGENT MESSAGE", 
        [
            "# Summary",
            "I have successfully implemented the requested changes.",
            "",
            "- Fixed the permission denied error in agentfs",
            "- Updated the interpose manager",
            "- Verified with tests",
            "",
            "Here is the corrected function:",
            "```rust",
            "fn handle_write(path: &Path) -> Result<()> {",
            "  if path.exists() {",
            "    // Handle existing path",
            "    println!(\"Path exists\");",
            "  }",
            "  Ok(())",
            "}",
            "```",
            "Ready for next task! 🚀"
        ],
        content_width,
        color_key="accent", # Green for success
        is_markdown=True
    )
    
    # 8. Task Completed (Active) - Instructions Card
    draw_instructions_card(content_width, active=True, show_title=False)
    
    # Future Events (Dimmed / Discarded)
    # Simulating a fork where subsequent events are dimmed
    
    draw_timeline_event(
        "14:25",
        "🔧",
        "RAN: cargo test",
        [
            color("running 1 test", "muted"),
            color("test tests::test_write ... ok", "muted")
        ],
        content_width,
        "border",
        command="cargo test",
        dimmed=True
    )
    
    draw_timeline_event(
        "14:25",
        "🧠",
        "Thought",
        [
            color("Tests passed. I'm confident in this solution.", "muted")
        ],
        content_width,
        "border",
        dimmed=True
    )
    
    print_line("")
    
    # Footer
    draw_footer()

if __name__ == "__main__":
    main()
