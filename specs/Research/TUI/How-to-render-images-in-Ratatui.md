Here’s the practical way to show images in a Ratatui app today.

# TL;DR

Use the community crate **`ratatui-image`**. It detects your terminal’s graphics protocol (Kitty / iTerm2 / Sixel) and falls back to Unicode “half-blocks” when necessary. You render an `Image` or `StatefulImage` widget inside your usual Ratatui `Terminal::draw` call. ([docs.rs][1])

---

## 1) Terminal support (important)

Different terminals support different graphics protocols:

- Kitty ≥ 0.28: Kitty graphics protocol ✔️
- iTerm2 (macOS): iTerm2 protocol ✔️
- Xterm / Foot / mlterm: Sixel ✔️ (for Xterm, run with `-ti 340` to enable Sixel)
- WezTerm: iTerm2 path is the most reliable; Kitty/Sixel can be glitchy
- Alacritty: no working graphics (there’s an old sixel fork, but it doesn’t clear graphics)

If none of those are available, the crate can **fallback to colored half-blocks** (no real pixels, but decent). ([GitHub][2])

---

## 2) Add dependencies

```toml
# Cargo.toml
[dependencies]
ratatui = "0.29"
crossterm = "0.27"   # or termion, match whatever backend you use
ratatui-image = "8"  # image widget with protocol detection
image = "0.25"       # used by ratatui-image to decode files
```

`ratatui-image` plugs into Ratatui and knows how to probe the terminal for protocol + font pixel size; it uses that to map image pixels to cell areas. ([docs.rs][1])

---

## 3) Minimal example (fits any Ratatui layout)

```rust
use ratatui::{backend::CrosstermBackend, Terminal, Frame};
use std::io;
use ratatui_image::{picker::Picker, StatefulImage, protocol::StatefulProtocol};

struct App {
    image: StatefulProtocol, // protocol + encoded data lives here
}

fn ui(f: &mut Frame<'_>, app: &mut App) {
    // Draw the image to whatever area you'd like (here: full frame)
    let img_widget = StatefulImage::default();
    img_widget.render(f.area(), f.buffer_mut(), &mut app.image);
    // (or f.render_stateful_widget(img_widget, f.area(), &mut app.image);)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // usual terminal setup omitted…

    // Ask the terminal what it can do (graphics protocol + font size).
    // This tries env vars and control sequences over stdio.
    let mut picker = Picker::from_query_stdio()?; // prefer this in real apps

    // Load an image with the `image` crate.
    let dyn_img = image::ImageReader::open("assets/picture.png")?.decode()?;

    // Create a protocol-backed state that resizes/encodes as needed.
    let image = picker.new_resize_protocol(dyn_img);

    let mut app = App { image };

    // In your draw loop:
    // terminal.draw(|f| ui(f, &mut app))?;
    // Handle any encoding result (helps surface errors):
    app.image.last_encoding_result().unwrap()?;

    Ok(())
}
```

This follows the crate’s “Quick start” pattern: create a `Picker`, decode with `image`, then render a `StatefulImage`. ([docs.rs][1])

---

## 4) Choosing the widget

- **`StatefulImage`** (recommended): resizes to the area, plays nice with stateful protocols (Kitty/iTerm2), and is more robust against overdraw artifacts. The resize/encode is blocking by default—offload to a worker thread if your images are big (see crate’s `examples/async.rs`).
- **`Image`** (stateless): never blocks the render loop, but doesn’t adapt to the area and can be glitchier with some protocols. ([docs.rs][1])

---

## 5) Features & backends

- Enable either **`crossterm`** or **`termion`** feature in `ratatui-image` to match your Ratatui backend.
- You can slim formats by disabling `image-defaults` and enabling only the formats you need. ([docs.rs][1])

---

## 6) Common gotchas

- **No image shows up?** Your terminal may not support graphics. Try Kitty or iTerm2, or ensure Xterm is started with Sixel support (`-ti 340`). ([GitHub][2])
- **Runs but flickers/overdraws?** Prefer `StatefulImage` over `Image`. ([docs.rs][1])
- **Performance stalls on large images?** Offload resize/encode to a thread (see `examples/async.rs`). ([docs.rs][1])

---

## 7) Extras & alternatives

- **Splash screens**: `ratatui-splash-screen` turns any image into a terminal splash screen widget. ([GitHub][3])
- **Ecosystem listing**: Ratatui’s third-party widgets page links `ratatui-image` and others. ([Ratatui][4])
- **How Ratatui renders** (if you’re curious about the draw pipeline). ([Ratatui][5])

If you want, tell me your target terminal (Kitty, iTerm2, WezTerm, etc.) and how big your images are—I can tailor the code to your exact setup (e.g., async pipeline, cropping/fit behavior).

[1]: https://docs.rs/ratatui-image/latest/ratatui_image/index.html 'ratatui_image - Rust'
[2]: https://github.com/benjajaja/ratatui-image 'GitHub - benjajaja/ratatui-image: Ratatui widget for rendering image graphics in terminals that support it'
[3]: https://github.com/orhun/ratatui-splash-screen?utm_source=chatgpt.com 'GitHub - orhun/ratatui-splash-screen: A Ratatui widget to turn any ...'
[4]: https://ratatui.rs/showcase/third-party-widgets/?utm_source=chatgpt.com 'Third Party Widgets Showcase - Ratatui'
[5]: https://ratatui.rs/concepts/rendering/under-the-hood/?utm_source=chatgpt.com 'Rendering under the hood - Ratatui'
