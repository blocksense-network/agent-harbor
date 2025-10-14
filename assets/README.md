# Application Assets

This directory contains application icons and resources for the Agent Harbor GUI.

## Icon Requirements

- **macOS**: `icon.icns` (1024x1024, multiple sizes)
- **Windows**: `icon.ico` (256x256, multiple sizes)
- **Linux**: `icon.png` (512x512 or larger)
- **Tray Icons**:
  - macOS: `tray-icon-Template.png` (22x22, template image for menu bar)
  - Windows/Linux: `tray-icon.png` (16x16, 32x32, 64x64)

## Creating Icons

To create production-quality icons from source artwork:

1. Create a high-resolution source image (1024x1024 or larger)
2. Use tools like:
   - macOS: `iconutil` or Icon Composer
   - Windows: Icon editors or `magick` (ImageMagick)
   - Linux: `convert` (ImageMagick) or `icotool`

For development, placeholder icons can be generated using ImageMagick:

```bash
# Create a simple colored square placeholder
convert -size 1024x1024 xc:'#667eea' \
  -gravity center \
  -pointsize 300 -fill white \
  -annotate +0+0 'AH' \
  icon-source.png

# Generate macOS icns
iconutil -c icns icon-source.iconset/

# Generate Windows ico
convert icon-source.png -define icon:auto-resize=256,128,64,48,32,16 icon.ico

# Generate Linux png
convert icon-source.png -resize 512x512 icon.png
```

## Current Status

This directory currently contains placeholder documentation.
Production icons will be added in a future milestone.
