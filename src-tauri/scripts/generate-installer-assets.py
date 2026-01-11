#!/usr/bin/env python3
"""
Generate installer assets for Sentinel
- macOS DMG background (660x400) - Clean, minimal design like standard macOS installers
- Windows NSIS header (150x57)
- Windows NSIS sidebar (164x314)
"""

from PIL import Image, ImageDraw
import os

# Output directory
OUTPUT_DIR = os.path.join(os.path.dirname(__file__), '..', 'installer-assets')
os.makedirs(OUTPUT_DIR, exist_ok=True)

# Colors - clean macOS-style palette
DMG_BG_COLOR = (245, 245, 247)  # Light gray like standard macOS DMGs
ARROW_COLOR = (160, 160, 165)   # Subtle gray arrow
ACCENT_COLOR = (249, 115, 22)   # Orange for Windows only

def create_dmg_background():
    """Create a clean macOS-style DMG background - light gray with subtle arrow."""
    width, height = 660, 400
    img = Image.new('RGB', (width, height), DMG_BG_COLOR)
    draw = ImageDraw.Draw(img)

    # Icon positions (Finder will render the actual icons here)
    # App icon center: x=180, y=190
    # Applications folder center: x=480, y=190

    # Arrow positioned between the two icons
    arrow_y = 190
    arrow_start_x = 260  # After app icon (with padding)
    arrow_end_x = 400    # Before Applications folder (with padding)

    # Draw a clean, visible arrow like standard installers
    # Arrow shaft - thicker for visibility
    shaft_thickness = 4
    draw.line([(arrow_start_x, arrow_y), (arrow_end_x - 12, arrow_y)],
              fill=ARROW_COLOR, width=shaft_thickness)

    # Arrow head - proportional triangle
    head_length = 14
    head_width = 10
    arrow_head = [
        (arrow_end_x, arrow_y),
        (arrow_end_x - head_length, arrow_y - head_width),
        (arrow_end_x - head_length, arrow_y + head_width),
    ]
    draw.polygon(arrow_head, fill=ARROW_COLOR)

    # Save
    output_path = os.path.join(OUTPUT_DIR, 'dmg-background.png')
    img.save(output_path, 'PNG')
    print(f"Created: {output_path}")
    return output_path


def create_nsis_header():
    """Create Windows installer header image (150x57) - clean gradient."""
    width, height = 150, 57
    img = Image.new('RGB', (width, height), ACCENT_COLOR)
    draw = ImageDraw.Draw(img)

    # Simple horizontal gradient from orange to slightly darker orange
    for x in range(width):
        shade = int(249 - (x / width) * 30)
        draw.line([(x, 0), (x, height)], fill=(shade, max(80, 115 - int(x/width*35)), 22))

    # Save as BMP (NSIS requires BMP format)
    output_path = os.path.join(OUTPUT_DIR, 'nsis-header.bmp')
    img.save(output_path, 'BMP')
    print(f"Created: {output_path}")
    return output_path


def create_nsis_sidebar():
    """Create Windows installer sidebar image (164x314) - clean gradient."""
    width, height = 164, 314
    img = Image.new('RGB', (width, height), ACCENT_COLOR)
    draw = ImageDraw.Draw(img)

    # Vertical gradient from orange at top to darker at bottom
    for y in range(height):
        factor = y / height
        r = int(249 - factor * 50)
        g = int(115 - factor * 40)
        b = int(22 + factor * 10)
        draw.line([(0, y), (width, y)], fill=(r, g, b))

    # Save as BMP
    output_path = os.path.join(OUTPUT_DIR, 'nsis-sidebar.bmp')
    img.save(output_path, 'BMP')
    print(f"Created: {output_path}")
    return output_path


if __name__ == '__main__':
    print("Generating minimal installer assets...")
    create_dmg_background()
    create_nsis_header()
    create_nsis_sidebar()
    print("Done!")
