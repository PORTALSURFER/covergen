//! Low-level software drawing helpers for the GUI panels.

use font8x8::UnicodeFonts;

/// Integer rectangle used by GUI layout and drawing routines.
#[derive(Clone, Copy)]
pub(crate) struct Rect {
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) w: i32,
    pub(crate) h: i32,
}

impl Rect {
    /// Construct one rectangle from integer position and size.
    pub(crate) const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }
}

/// Fill one rectangle in `frame` using one ARGB color.
pub(crate) fn fill_rect(frame: &mut [u32], width: usize, height: usize, rect: Rect, color: u32) {
    let y0 = rect.y.max(0) as usize;
    let y1 = (rect.y + rect.h).min(height as i32).max(0) as usize;
    let x0 = rect.x.max(0) as usize;
    let x1 = (rect.x + rect.w).min(width as i32).max(0) as usize;

    for y in y0..y1 {
        let row = y * width;
        for x in x0..x1 {
            frame[row + x] = color;
        }
    }
}

/// Draw rectangle border.
pub(crate) fn stroke_rect(frame: &mut [u32], width: usize, height: usize, rect: Rect, color: u32) {
    let x0 = rect.x;
    let y0 = rect.y;
    let x1 = rect.x + rect.w - 1;
    let y1 = rect.y + rect.h - 1;
    draw_line(frame, width, height, x0, y0, x1, y0, color);
    draw_line(frame, width, height, x1, y0, x1, y1, color);
    draw_line(frame, width, height, x1, y1, x0, y1, color);
    draw_line(frame, width, height, x0, y1, x0, y0, color);
}

/// Draw one Bresenham line segment.
pub(crate) fn draw_line(
    frame: &mut [u32],
    width: usize,
    height: usize,
    mut x0: i32,
    mut y0: i32,
    x1: i32,
    y1: i32,
    color: u32,
) {
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;

    loop {
        put_pixel(frame, width, height, x0, y0, color);
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}

/// Draw text using builtin 8x8 bitmap glyphs.
pub(crate) fn draw_text(
    frame: &mut [u32],
    width: usize,
    height: usize,
    x: i32,
    y: i32,
    text: &str,
    color: u32,
) {
    let mut cursor = x;
    for ch in text.chars() {
        draw_char(frame, width, height, cursor, y, ch, color);
        cursor += 8;
    }
}

fn draw_char(frame: &mut [u32], width: usize, height: usize, x: i32, y: i32, ch: char, color: u32) {
    let glyph = font8x8::BASIC_FONTS
        .get(ch)
        .or_else(|| font8x8::BASIC_FONTS.get('?'));
    let Some(glyph) = glyph else {
        return;
    };

    for (row, bits) in glyph.iter().enumerate() {
        for col in 0..8 {
            if ((bits >> col) & 1) == 1 {
                put_pixel(frame, width, height, x + col, y + row as i32, color);
            }
        }
    }
}

fn put_pixel(frame: &mut [u32], width: usize, height: usize, x: i32, y: i32, color: u32) {
    if x < 0 || y < 0 {
        return;
    }
    let x = x as usize;
    let y = y as usize;
    if x >= width || y >= height {
        return;
    }
    frame[y * width + x] = color;
}
