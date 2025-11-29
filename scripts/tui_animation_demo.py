#!/usr/bin/env python3
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

import time
import sys
import math
import shutil
import textwrap

# Catppuccin Mocha Colors
COLORS = {
    "bg": (17, 17, 27),
    "text": (205, 214, 244),
    "muted": (127, 132, 156),
    "primary": (137, 180, 250),
    "accent": (166, 227, 161),
    "border": (69, 71, 90),
}

MARGIN_X = 4

def color_code(r, g, b):
    return f"\033[38;2;{int(r)};{int(g)};{int(b)}m"

def interpolate_color(c1, c2, t):
    r = c1[0] + (c2[0] - c1[0]) * t
    g = c1[1] + (c2[1] - c1[1]) * t
    b = c1[2] + (c2[2] - c1[2]) * t
    return (r, g, b)

def clear_lines(n):
    for _ in range(n):
        sys.stdout.write("\033[F\033[K")

def get_visual_len(text):
    import re
    ansi_escape = re.compile(r'\x1B(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~])')
    clean_text = ansi_escape.sub('', text)
    length = len(clean_text)
    for char in clean_text:
        if char in "🧠🔧📝":
            length += 1
    return length

SHOW_TIMELINE = False

SHOW_TIMELINE = False

def main():
    print("\033[?25l") # Hide cursor
    cols, _ = shutil.get_terminal_size()
    content_width = min(80, cols - 2 * MARGIN_X)
    
    spinner_frames = ["⠋", "⠙", "⠹", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
    
    print("\n")
    print(" " * MARGIN_X + "Agent Activity Timeline Animation (Ctrl+C to exit)")
    print("\n")
    
    # Static part of timeline
    if SHOW_TIMELINE:
        print(" " * MARGIN_X + f"{color_code(*COLORS['muted'])}  ◷ 14:30:05\033[0m")
        print(" " * MARGIN_X + f"{color_code(*COLORS['muted'])}  │\033[0m")
    else:
        print(" " * MARGIN_X + f"{color_code(*COLORS['muted'])}◷ 14:30:05\033[0m")
    
    # Previous event (static)
    static_title_text = "THOUGHT"
    static_content = "I need to verify the fix works."
    
    # Card width
    if SHOW_TIMELINE:
        card_w = content_width - 6
    else:
        card_w = content_width
    
    # Parts
    start_marker = "╭─"
    title_block_inner = static_title_text
    title_block = "┤ " + title_block_inner + " ├"
    close_block = "┤ ▼ ├─╮"
    
    len_start = get_visual_len(start_marker)
    len_title_block = get_visual_len(title_block)
    len_close_block = get_visual_len(close_block)
    
    dash_len = card_w - len_start - len_title_block - len_close_block
    
    # Line -1 (Caps)
    title_cap_str = "╭" + "─" * (len_title_block - 2) + "╮"
    close_cap_str = "╭───╮"
    
    line_minus_1 = " " * MARGIN_X
    if SHOW_TIMELINE:
        line_minus_1 += f"{color_code(*COLORS['muted'])}  │\033[0m" + " " * 3
        
    line_minus_1 += " " * len_start + f"{color_code(*COLORS['border'])}{title_cap_str}\033[0m"
    
    pad_to_close = (card_w - 7) - (len_start + len_title_block)
    if pad_to_close < 0: pad_to_close = 0
    
    line_minus_1 += " " * pad_to_close + f"{color_code(*COLORS['border'])}{close_cap_str}\033[0m"
    print(line_minus_1)
    
    # Render Top (Border color for frame, Border color for title since it's Thought)
    card_top = (
        color_code(*COLORS["border"]) + start_marker + 
        color_code(*COLORS["border"]) + "┤ " + 
        color_code(*COLORS["border"]) + title_block_inner + 
        color_code(*COLORS["border"]) + " ├" + 
        "─" * dash_len + close_block + "\033[0m"
    )
    
    prefix = " " * MARGIN_X
    if SHOW_TIMELINE:
        prefix += f"{color_code(*COLORS['muted'])}  ├─ \033[0m"
        
    print(prefix + card_top)
    
    # Spacer Line (Line +1) - Dedicated for Bottom Cap
    title_bot_cap = "╰" + "─" * (len_title_block - 2) + "╯"
    close_bot_cap = "╰───╯"
    
    pad_mid = (card_w - 7) - (2 + len_title_block)
    if pad_mid < 0: pad_mid = 0
    
    line_plus_1 = " " * MARGIN_X
    if SHOW_TIMELINE:
        line_plus_1 += f"{color_code(*COLORS['muted'])}  │  \033[0m"
        
    line_plus_1 += (
        f"{color_code(*COLORS['border'])}│ \033[0m" + 
        f"{color_code(*COLORS['border'])}{title_bot_cap}\033[0m" + 
        " " * pad_mid + 
        f"{color_code(*COLORS['border'])}{close_bot_cap}\033[0m" + 
        f"{color_code(*COLORS['border'])} │\033[0m"
    )
    print(line_plus_1)
    
    # Content
    padding = card_w - 4 - get_visual_len(static_content)
    
    prefix = " " * MARGIN_X
    if SHOW_TIMELINE:
        prefix += f"{color_code(*COLORS['muted'])}  │  \033[0m"
        
    print(prefix + f"{color_code(*COLORS['border'])}│ \033[0m{color_code(*COLORS['text'])}{static_content}\033[0m" + " " * padding + f"{color_code(*COLORS['border'])} │\033[0m")
    
    # Bottom
    prefix = " " * MARGIN_X
    if SHOW_TIMELINE:
        prefix += f"{color_code(*COLORS['muted'])}  │  \033[0m"
        
    print(prefix + f"{color_code(*COLORS['border'])}╰" + "─" * (card_w - 2) + "╯\033[0m")
    
    if SHOW_TIMELINE:
        print(" " * MARGIN_X + f"{color_code(*COLORS['muted'])}  │\033[0m")
    
    try:
        start_time = time.time()
        
        while True:
            elapsed = time.time() - start_time
            cycle_time = elapsed % 8.0
            
            # Draw Hero Card
            hero_lines_count = 0
            
            if cycle_time < 3.0:
                # STATE: THINKING
                pulse_t = (math.sin(elapsed * 4) + 1) / 2
                border_rgb = interpolate_color(COLORS["primary"], COLORS["border"], pulse_t)
                border_color = color_code(*border_rgb)
                
                hero_title_text = "THINKING"
                hero_content = "Planning the test command to verify the fix..."
                
                # Parts
                start_marker = "╭─"
                title_block_inner = hero_title_text
                title_block = "┤ " + title_block_inner + " ├"
                close_block = "┤ ▼ ├─╮"
                
                len_start = get_visual_len(start_marker)
                len_title_block = get_visual_len(title_block)
                len_close_block = get_visual_len(close_block)
                
                dash_len = content_width - len_start - len_title_block - len_close_block
                
                # Colors
                title_rgb = interpolate_color(COLORS["primary"], COLORS["muted"], pulse_t)
                title_color = color_code(*title_rgb)
                frame_color = color_code(*COLORS["border"])
                
                # Top Cap
                title_cap_str = "╭" + "─" * (len_title_block - 2) + "╮"
                close_cap_str = "╭───╮"
                
                line_minus_1 = " " * MARGIN_X + " " * len_start + f"{frame_color}{title_cap_str}\033[0m"
                pad_to_close = (content_width - 7) - (len_start + len_title_block)
                if pad_to_close < 0: pad_to_close = 0
                line_minus_1 += " " * pad_to_close + f"{frame_color}{close_cap_str}\033[0m"
                
                sys.stdout.write(line_minus_1 + "\n")
                hero_lines_count += 1
                
                top_line = (
                    frame_color + start_marker + 
                    "┤ " + title_color + title_block_inner + frame_color + " ├" + 
                    "─" * dash_len + close_block + "\033[0m"
                )
                
                sys.stdout.write(" " * MARGIN_X + top_line + "\n")
                hero_lines_count += 1
                
                # Spacer Line (Line +1)
                title_bot_cap = "╰" + "─" * (len_title_block - 2) + "╯"
                close_bot_cap = "╰───╯"
                pad_mid = (content_width - 7) - (2 + len_title_block)
                if pad_mid < 0: pad_mid = 0
                
                line_plus_1 = (
                    " " * MARGIN_X + 
                    f"{frame_color}│ \033[0m" + 
                    f"{frame_color}{title_bot_cap}\033[0m" + 
                    " " * pad_mid + 
                    f"{frame_color}{close_bot_cap}\033[0m" + 
                    f"{frame_color} │\033[0m"
                )
                sys.stdout.write(line_plus_1 + "\n")
                hero_lines_count += 1
                
                # Content
                padding = content_width - 4 - get_visual_len(hero_content)
                sys.stdout.write(" " * MARGIN_X + f"{frame_color}│ \033[0m{color_code(*COLORS['text'])}{hero_content}\033[0m" + " " * padding + f"{frame_color} │\033[0m\n")
                hero_lines_count += 1
                
                # Bottom
                sys.stdout.write(" " * MARGIN_X + f"{frame_color}╰" + "─" * (content_width - 2) + "╯\033[0m\n")
                hero_lines_count += 1
                
                # Timeline Spacer (Empty space where tool will appear)
                sys.stdout.write("\n" * 3) 
                hero_lines_count += 3
                
            else:
                # STATE: RUNNING TOOL
                hero_title_text = "RUNNING TOOL"
                hero_content = "Executing cargo test..."
                
                # Colors
                title_color = color_code(*COLORS["primary"])
                frame_color = color_code(*COLORS["border"])
                
                # Parts
                start_marker = "╭─"
                title_block_inner = hero_title_text
                title_block = "┤ " + title_block_inner + " ├"
                close_block = "┤ ▼ ├─╮"
                
                len_start = get_visual_len(start_marker)
                len_title_block = get_visual_len(title_block)
                len_close_block = get_visual_len(close_block)
                
                dash_len = content_width - len_start - len_title_block - len_close_block
                
                # Top Cap
                title_cap_str = "╭" + "─" * (len_title_block - 2) + "╮"
                close_cap_str = "╭───╮"
                
                line_minus_1 = " " * MARGIN_X + " " * len_start + f"{frame_color}{title_cap_str}\033[0m"
                pad_to_close = (content_width - 7) - (len_start + len_title_block)
                if pad_to_close < 0: pad_to_close = 0
                line_minus_1 += " " * pad_to_close + f"{frame_color}{close_cap_str}\033[0m"
                
                sys.stdout.write(line_minus_1 + "\n")
                hero_lines_count += 1
                
                top_line = (
                    frame_color + start_marker + 
                    "┤ " + title_color + title_block_inner + frame_color + " ├" + 
                    "─" * dash_len + close_block + "\033[0m"
                )
                
                sys.stdout.write(" " * MARGIN_X + top_line + "\n")
                hero_lines_count += 1
                
                # Spacer Line (Line +1)
                title_bot_cap = "╰" + "─" * (len_title_block - 2) + "╯"
                close_bot_cap = "╰───╯"
                pad_mid = (content_width - 7) - (2 + len_title_block)
                if pad_mid < 0: pad_mid = 0
                
                line_plus_1 = (
                    " " * MARGIN_X + 
                    f"{frame_color}│ \033[0m" + 
                    f"{frame_color}{title_bot_cap}\033[0m" + 
                    " " * pad_mid + 
                    f"{frame_color}{close_bot_cap}\033[0m" + 
                    f"{frame_color} │\033[0m"
                )
                sys.stdout.write(line_plus_1 + "\n")
                hero_lines_count += 1
                
                # Content
                padding = content_width - 4 - get_visual_len(hero_content)
                sys.stdout.write(" " * MARGIN_X + f"{frame_color}│ \033[0m{color_code(*COLORS['text'])}{hero_content}\033[0m" + " " * padding + f"{frame_color} │\033[0m\n")
                hero_lines_count += 1
                
                # Bottom
                sys.stdout.write(" " * MARGIN_X + f"{frame_color}╰" + "─" * (content_width - 2) + "╯\033[0m\n")
                hero_lines_count += 1
                
                # Timeline Active Tool Card
                spinner_idx = int(elapsed * 10) % len(spinner_frames)
                spinner = spinner_frames[spinner_idx]
                
                # Timeline Tool Card
                # "TOOLS cargo test ( ■ )"
                tool_category = "TOOLS"
                tool_name = "cargo test"
                stop_btn_str = f"{color_code(*COLORS['muted'])} ( {color_code(*COLORS['error'])}■{color_code(*COLORS['muted'])} ) \033[0m"
                stop_btn_len = 5
                
                if SHOW_TIMELINE:
                    card_w = content_width - 6 
                else:
                    card_w = content_width
                
                start_marker = "╭─"
                
                # Construct title inner
                title_inner_colored = (
                    f"{color_code(*COLORS['primary'])}{tool_category} \033[0m" + 
                    f"{color_code(*COLORS['primary'])}{tool_name}\033[0m" + 
                    stop_btn_str
                )
                title_inner_len = len(tool_category) + 1 + len(tool_name) + stop_btn_len
                
                title_block_colored = f"{frame_color}┤ {title_inner_colored}{frame_color} ├\033[0m"
                len_title_block = 2 + title_inner_len + 2
                
                close_block = "┤ ▼ ├─╮"
                len_start = get_visual_len(start_marker)
                len_close_block = get_visual_len(close_block)
                
                dash_len = card_w - len_start - len_title_block - len_close_block
                
                # Colors
                frame_color = color_code(*COLORS["border"])
                
                # Top Cap
                title_cap_str = "╭" + "─" * (len_title_block - 2) + "╮"
                close_cap_str = "╭───╮"
                
                line_minus_1 = " " * MARGIN_X
                if SHOW_TIMELINE:
                    line_minus_1 += f"{color_code(*COLORS['muted'])}  │\033[0m" + " " * 3
                
                line_minus_1 += " " * len_start + f"{frame_color}{title_cap_str}\033[0m"
                pad_to_close = (card_w - 7) - (len_start + len_title_block)
                if pad_to_close < 0: pad_to_close = 0
                line_minus_1 += " " * pad_to_close + f"{frame_color}{close_cap_str}\033[0m"
                
                sys.stdout.write(line_minus_1 + "\n")
                
                card_top = (
                    frame_color + start_marker + 
                    title_block_colored + 
                    frame_color + "─" * dash_len + close_block + "\033[0m"
                )
                
                prefix = " " * MARGIN_X
                if SHOW_TIMELINE:
                    prefix += f"{color_code(*COLORS['muted'])}  ├─ \033[0m"
                
                sys.stdout.write(prefix + card_top + "\n")
                
                # Spacer Line (Line +1)
                title_bot_cap = "╰" + "─" * (len_title_block - 2) + "╯"
                close_bot_cap = "╰───╯"
                pad_mid = (card_w - 7) - (2 + len_title_block)
                if pad_mid < 0: pad_mid = 0
                
                line_plus_1 = " " * MARGIN_X
                if SHOW_TIMELINE:
                    line_plus_1 += f"{color_code(*COLORS['muted'])}  │  \033[0m"
                
                line_plus_1 += (
                    f"{frame_color}│ \033[0m" + 
                    f"{frame_color}{title_bot_cap}\033[0m" + 
                    " " * pad_mid + 
                    f"{frame_color}{close_bot_cap}\033[0m" + 
                    f"{frame_color} │\033[0m"
                )
                sys.stdout.write(line_plus_1 + "\n")
                
                content = "Running tests for ah-fs-snapshots..."
                padding = card_w - 4 - get_visual_len(content)
                
                prefix = " " * MARGIN_X
                if SHOW_TIMELINE:
                    prefix += f"{color_code(*COLORS['muted'])}  │  \033[0m"
                
                sys.stdout.write(prefix + f"{frame_color}│ \033[0m{color_code(*COLORS['text'])}{content}\033[0m" + " " * padding + f"{frame_color} │\033[0m\n")
                
                prefix = " " * MARGIN_X
                if SHOW_TIMELINE:
                    prefix += f"{color_code(*COLORS['muted'])}  │  \033[0m"
                
                sys.stdout.write(prefix + f"{frame_color}╰" + "─" * (card_w - 2) + "╯\033[0m\n")
                
                hero_lines_count += 5 # Added extra line for top cap AND spacer line

            
            # Reset cursor to redraw
            clear_lines(hero_lines_count)
            
            time.sleep(0.05)
            
    except KeyboardInterrupt:
        print("\n" * 10) # Move past the animation area
        print("\033[?25h") # Show cursor
        print("Exiting...")

            
            # Reset cursor to redraw
            clear_lines(hero_lines_count)
            
            time.sleep(0.05)
            
    except KeyboardInterrupt:
        print("\n" * 10) # Move past the animation area
        print("\033[?25h") # Show cursor
        print("Exiting...")

            
            # Reset cursor to redraw
            clear_lines(hero_lines_count)
            
            time.sleep(0.05)
            
    except KeyboardInterrupt:
        print("\n" * 10) # Move past the animation area
        print("\033[?25h") # Show cursor
        print("Exiting...")

if __name__ == "__main__":
    main()
