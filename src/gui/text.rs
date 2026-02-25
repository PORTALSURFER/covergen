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
const MIN_FONT_SIZE_PX: f32 = 4.0;
const MAX_FONT_SIZE_PX: f32 = 48.0;
const FONT_SIZE_QUANT_STEP: f32 = 4.0;
const GLYPH_COVERAGE_THRESHOLD: u8 = 96;
const TAB_SPACES: i32 = 4;

/// Scaled line-layout metrics for one rendered text run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GuiTextMetrics {
    pub(crate) baseline_px: i32,
    pub(crate) line_height_px: i32,
}

/// Cached text renderer that emits glyph pixels as scene rectangles.
pub(crate) struct GuiTextRenderer {
    font: Option<Font<'static>>,
    glyph_cache: HashMap<GlyphCacheKey, GlyphBitmap>,
}

impl Default for GuiTextRenderer {
    fn default() -> Self {
        let font = Font::try_from_bytes(FONT_BYTES);
        Self {
            font,
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
        self.push_text_scaled(out, x, y, text, color, 1.0);
    }

    /// Append text rectangles at `(x, y)` using a scale multiplier.
    ///
    /// `scale` is relative to the baseline font size (`12px`).
    pub(crate) fn push_text_scaled(
        &mut self,
        out: &mut Vec<ColoredRect>,
        x: i32,
        y: i32,
        text: &str,
        color: Color,
        scale: f32,
    ) {
        if self.font.is_none() || text.is_empty() {
            return;
        }
        let size_key = quantized_font_size(scale);
        let metrics = self.metrics_scaled(scale);
        let mut cursor_x = x;
        let mut cursor_y = y;
        for ch in text.chars() {
            if ch == '\n' {
                cursor_x = x;
                cursor_y += metrics.line_height_px;
                continue;
            }
            if ch == '\t' {
                cursor_x += TAB_SPACES * self.space_advance(size_key);
                continue;
            }
            let glyph = self.cached_glyph(ch, size_key, metrics.baseline_px);
            push_glyph_runs(out, cursor_x, cursor_y, glyph, color);
            cursor_x += glyph.advance_px.max(1);
        }
    }

    /// Return line metrics for the provided text scale.
    pub(crate) fn metrics_scaled(&self, scale: f32) -> GuiTextMetrics {
        let size_key = quantized_font_size(scale);
        let glyph_scale = Scale::uniform(font_size_from_key(size_key));
        let (baseline_px, line_height_px) = self
            .font
            .as_ref()
            .map(|loaded| line_metrics(loaded, glyph_scale))
            .unwrap_or((10, 14));
        GuiTextMetrics {
            baseline_px,
            line_height_px,
        }
    }

    /// Return measured single-line width for `text` at the provided scale.
    pub(crate) fn measure_text_width(&self, text: &str, scale: f32) -> i32 {
        if text.is_empty() {
            return 0;
        }
        let size_key = quantized_font_size(scale);
        let mut line_width = 0;
        let mut max_width = 0;
        for ch in text.chars() {
            if ch == '\n' {
                max_width = max_width.max(line_width);
                line_width = 0;
                continue;
            }
            if ch == '\t' {
                line_width += TAB_SPACES * self.space_advance(size_key);
                continue;
            }
            line_width += self.glyph_advance(ch, size_key);
        }
        max_width.max(line_width)
    }

    /// Return measured width for one character at the provided scale.
    pub(crate) fn measure_char_width(&self, ch: char, scale: f32) -> i32 {
        let size_key = quantized_font_size(scale);
        if ch == '\n' {
            return 0;
        }
        if ch == '\t' {
            return TAB_SPACES * self.space_advance(size_key);
        }
        self.glyph_advance(ch, size_key)
    }

    fn space_advance(&self, size_key: u16) -> i32 {
        self.glyph_advance(' ', size_key).max(1)
    }

    fn glyph_advance(&self, ch: char, size_key: u16) -> i32 {
        let Some(font) = &self.font else {
            return 8;
        };
        let key = self.lookup_char(ch);
        let scale = Scale::uniform(font_size_from_key(size_key));
        font.glyph(key)
            .scaled(scale)
            .h_metrics()
            .advance_width
            .ceil()
            .max(1.0) as i32
    }

    /// Return text caret x-offset for a UTF-8 byte cursor at the provided scale.
    pub(crate) fn cursor_offset(&self, text: &str, cursor: usize, scale: f32) -> i32 {
        let clamped = cursor.min(text.len());
        self.measure_text_width(&text[..clamped], scale)
    }

    fn cached_glyph(&mut self, ch: char, size_key: u16, baseline_px: i32) -> &GlyphBitmap {
        let key = self.lookup_char(ch);
        let cache_key = GlyphCacheKey {
            ch: key,
            size_key,
            baseline_px,
        };
        if !self.glyph_cache.contains_key(&cache_key) {
            let glyph = self.rasterize_glyph(key, size_key, baseline_px);
            self.glyph_cache.insert(cache_key, glyph);
        }
        self.glyph_cache
            .get(&cache_key)
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

    fn rasterize_glyph(&self, ch: char, size_key: u16, baseline_px: i32) -> GlyphBitmap {
        let Some(font) = &self.font else {
            return GlyphBitmap::empty(8);
        };
        let scale = Scale::uniform(font_size_from_key(size_key));
        let scaled = font.glyph(ch).scaled(scale);
        let advance_px = scaled.h_metrics().advance_width.ceil() as i32;
        let positioned = scaled.positioned(point(0.0, baseline_px as f32));
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

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct GlyphCacheKey {
    ch: char,
    size_key: u16,
    baseline_px: i32,
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

fn quantized_font_size(scale: f32) -> u16 {
    let px = (FONT_SIZE_PX * scale).clamp(MIN_FONT_SIZE_PX, MAX_FONT_SIZE_PX);
    (px * FONT_SIZE_QUANT_STEP).round() as u16
}

fn font_size_from_key(size_key: u16) -> f32 {
    size_key as f32 / FONT_SIZE_QUANT_STEP
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
    use super::{
        font_size_from_key, quantized_font_size, GuiTextRenderer, MAX_FONT_SIZE_PX,
        MIN_FONT_SIZE_PX,
    };
    use crate::gui::scene::Color;

    #[test]
    fn cached_renderer_emits_rects_for_ascii_text() {
        let mut text = GuiTextRenderer::default();
        let mut rects = Vec::new();
        text.push_text(&mut rects, 20, 10, "Output", Color::argb(0xFFFFFFFF));
        assert!(!rects.is_empty());
    }

    #[test]
    fn scaled_text_expands_rect_coverage_when_zoomed_in() {
        let mut text = GuiTextRenderer::default();
        let mut base_rects = Vec::new();
        text.push_text_scaled(
            &mut base_rects,
            0,
            0,
            "W",
            Color::argb(0xFFFFFFFF),
            1.0,
        );
        let mut large_rects = Vec::new();
        text.push_text_scaled(
            &mut large_rects,
            0,
            0,
            "W",
            Color::argb(0xFFFFFFFF),
            2.0,
        );
        let base_width: i32 = base_rects.iter().map(|rect| rect.rect.w).sum();
        let large_width: i32 = large_rects.iter().map(|rect| rect.rect.w).sum();
        assert!(large_width > base_width);
    }

    #[test]
    fn quantized_font_size_is_clamped_to_safe_bounds() {
        assert!(font_size_from_key(quantized_font_size(0.01)) >= MIN_FONT_SIZE_PX);
        assert!(font_size_from_key(quantized_font_size(99.0)) <= MAX_FONT_SIZE_PX);
    }
}
