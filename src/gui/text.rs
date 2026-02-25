//! Lightweight GUI text rasterization from JetBrains Mono.
//!
//! This module keeps GUI text in the existing rectangle scene pipeline by
//! rasterizing glyphs from `assets/JetBrainsMono.ttf` and emitting lit spans
//! as `ColoredRect` rows. Glyph bitmaps are cached per character.

use std::collections::HashMap;

use rusttype::{point, Font, Scale};

use super::geometry::Rect;
use super::scene::{Color, ColoredRect};

const FONT_BYTES: &[u8] = include_bytes!("../../assets/JetBrainsMono.ttf");
const FONT_SIZE_PX: f32 = 12.0;
const GLYPH_COVERAGE_THRESHOLD: u8 = 96;
const TAB_SPACES: i32 = 4;

/// Cached text renderer that emits glyph pixels as scene rectangles.
pub(crate) struct GuiTextRenderer {
    font: Option<Font<'static>>,
    scale: Scale,
    baseline_px: i32,
    line_height_px: i32,
    glyph_cache: HashMap<char, GlyphBitmap>,
}

impl Default for GuiTextRenderer {
    fn default() -> Self {
        let scale = Scale::uniform(FONT_SIZE_PX);
        let font = Font::try_from_bytes(FONT_BYTES);
        let (baseline_px, line_height_px) = font
            .as_ref()
            .map(|loaded| line_metrics(loaded, scale))
            .unwrap_or((10, 14));
        Self {
            font,
            scale,
            baseline_px,
            line_height_px,
            glyph_cache: HashMap::new(),
        }
    }
}

impl GuiTextRenderer {
    /// Append text rectangles at `(x, y)` using top-left anchored line layout.
    pub(crate) fn push_text(
        &mut self,
        out: &mut Vec<ColoredRect>,
        x: i32,
        y: i32,
        text: &str,
        color: Color,
    ) {
        if self.font.is_none() || text.is_empty() {
            return;
        }
        let mut cursor_x = x;
        let mut cursor_y = y;
        for ch in text.chars() {
            if ch == '\n' {
                cursor_x = x;
                cursor_y += self.line_height_px;
                continue;
            }
            if ch == '\t' {
                cursor_x += TAB_SPACES * self.space_advance();
                continue;
            }
            let glyph = self.cached_glyph(ch);
            push_glyph_runs(out, cursor_x, cursor_y, glyph, color);
            cursor_x += glyph.advance_px.max(1);
        }
    }

    fn space_advance(&mut self) -> i32 {
        self.cached_glyph(' ').advance_px.max(1)
    }

    fn cached_glyph(&mut self, ch: char) -> &GlyphBitmap {
        let key = self.lookup_char(ch);
        if !self.glyph_cache.contains_key(&key) {
            let glyph = self.rasterize_glyph(key);
            self.glyph_cache.insert(key, glyph);
        }
        self.glyph_cache
            .get(&key)
            .expect("glyph must exist after cache insert")
    }

    fn lookup_char(&self, ch: char) -> char {
        let Some(font) = &self.font else {
            return ch;
        };
        if has_renderable_glyph(font, ch) {
            ch
        } else {
            '?'
        }
    }

    fn rasterize_glyph(&self, ch: char) -> GlyphBitmap {
        let Some(font) = &self.font else {
            return GlyphBitmap::empty(8);
        };
        let scaled = font.glyph(ch).scaled(self.scale);
        let advance_px = scaled.h_metrics().advance_width.ceil() as i32;
        let positioned = scaled.positioned(point(0.0, self.baseline_px as f32));
        let Some(bounds) = positioned.pixel_bounding_box() else {
            return GlyphBitmap::empty(advance_px.max(1));
        };
        let width = bounds.width().max(0) as usize;
        let height = bounds.height().max(0) as usize;
        let mut coverage = vec![0u8; width.saturating_mul(height)];
        positioned.draw(|px, py, value| {
            let idx = py as usize * width + px as usize;
            coverage[idx] = (value * 255.0).round() as u8;
        });
        GlyphBitmap {
            x_offset_px: bounds.min.x,
            y_offset_px: bounds.min.y,
            width_px: width as i32,
            height_px: height as i32,
            advance_px: advance_px.max(1),
            coverage,
        }
    }
}

#[derive(Clone)]
struct GlyphBitmap {
    x_offset_px: i32,
    y_offset_px: i32,
    width_px: i32,
    height_px: i32,
    advance_px: i32,
    coverage: Vec<u8>,
}

impl GlyphBitmap {
    fn empty(advance_px: i32) -> Self {
        Self {
            x_offset_px: 0,
            y_offset_px: 0,
            width_px: 0,
            height_px: 0,
            advance_px,
            coverage: Vec::new(),
        }
    }
}

fn line_metrics(font: &Font<'_>, scale: Scale) -> (i32, i32) {
    let metrics = font.v_metrics(scale);
    let baseline_px = metrics.ascent.ceil() as i32;
    let line_height = metrics.ascent - metrics.descent + metrics.line_gap;
    (baseline_px.max(1), line_height.ceil() as i32)
}

fn has_renderable_glyph(font: &Font<'_>, ch: char) -> bool {
    ch == ' ' || font.glyph(ch).id().0 != 0
}

fn push_glyph_runs(out: &mut Vec<ColoredRect>, x: i32, y: i32, glyph: &GlyphBitmap, color: Color) {
    if glyph.width_px <= 0 || glyph.height_px <= 0 {
        return;
    }
    let width = glyph.width_px as usize;
    let height = glyph.height_px as usize;
    for row in 0..height {
        let row_start = row * width;
        push_row_runs(
            out,
            x + glyph.x_offset_px,
            y + glyph.y_offset_px + row as i32,
            &glyph.coverage[row_start..row_start + width],
            color,
        );
    }
}

fn push_row_runs(out: &mut Vec<ColoredRect>, x: i32, y: i32, row: &[u8], color: Color) {
    let mut run_start = None;
    for (col, coverage) in row.iter().enumerate() {
        let lit = *coverage >= GLYPH_COVERAGE_THRESHOLD;
        if lit && run_start.is_none() {
            run_start = Some(col as i32);
        } else if !lit {
            if let Some(start) = run_start.take() {
                flush_run(out, x, y, start, col as i32, color);
            }
        }
    }
    if let Some(start) = run_start {
        flush_run(out, x, y, start, row.len() as i32, color);
    }
}

fn flush_run(out: &mut Vec<ColoredRect>, x: i32, y: i32, start: i32, end: i32, color: Color) {
    let width = end - start;
    if width <= 0 {
        return;
    }
    out.push(ColoredRect {
        rect: Rect::new(x + start, y, width, 1),
        color,
    });
}

#[cfg(test)]
mod tests {
    use super::GuiTextRenderer;
    use crate::gui::scene::Color;

    #[test]
    fn cached_renderer_emits_rects_for_ascii_text() {
        let mut text = GuiTextRenderer::default();
        let mut rects = Vec::new();
        text.push_text(&mut rects, 20, 10, "Output", Color::argb(0xFFFFFFFF));
        assert!(!rects.is_empty());
    }
}
