mod bitmap;

use bitmap::FONT8X8;
use anyhow::Error;
use crate::color::Color;
use crate::framebuffer::Framebuffer;
use crate::geom::Point;
use crate::pt;

// Sizes are in 1/64 of a point, matching the values the views already pass.
pub const FONT_SIZES: [u32; 3] = [349, 524, 629];
pub const KEYBOARD_FONT_SIZES: [u32; 2] = [337, 843];
pub const DISPLAY_FONT_SIZE: u32 = 2516;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Family {
    SansSerif,
    Serif,
    Monospace,
    Keyboard,
    Display,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Variant {
    Regular,
    Italic,
    Bold,
    BoldItalic,
}

pub struct Style {
    family: Family,
    variant: Variant,
    pub size: u32,
}

pub const NORMAL_STYLE: Style = Style {
    family: Family::SansSerif,
    variant: Variant::Regular,
    size: FONT_SIZES[1],
};

pub const SPECIAL_STYLE: Style = Style {
    family: Family::SansSerif,
    variant: Variant::Italic,
    size: FONT_SIZES[1],
};

pub const KBD_CHAR: Style = Style {
    family: Family::Keyboard,
    variant: Variant::Regular,
    size: KEYBOARD_FONT_SIZES[1],
};

pub const KBD_LABEL: Style = Style {
    family: Family::Keyboard,
    variant: Variant::Regular,
    size: FONT_SIZES[0],
};

pub const DISPLAY_STYLE: Style = Style {
    family: Family::Display,
    variant: Variant::Regular,
    size: DISPLAY_FONT_SIZE,
};

pub const MD_AUTHOR: Style = Style {
    family: Family::Serif,
    variant: Variant::Regular,
    size: FONT_SIZES[1],
};

pub const MD_YEAR: Style = NORMAL_STYLE;

pub const MD_KIND: Style = Style {
    family: Family::SansSerif,
    variant: Variant::Bold,
    size: FONT_SIZES[0],
};

pub const MD_SIZE: Style = Style {
    family: Family::SansSerif,
    variant: Variant::Regular,
    size: FONT_SIZES[0],
};

const ELLIPSIS: u32 = '…' as u32;
const ELLIPSIS_ROWS: [u8; 8] = [0x00, 0x00, 0x00, 0x00, 0x00, 0x49, 0x49, 0x00];

#[derive(Debug, Copy, Clone)]
struct GlyphPlan {
    codepoint: u32,
    advance: Point,
}

#[derive(Debug, Clone)]
pub struct RenderPlan {
    pub width: i32,
    glyphs: Vec<GlyphPlan>,
}

impl Default for RenderPlan {
    fn default() -> RenderPlan {
        RenderPlan {
            width: 0,
            glyphs: Vec::new(),
        }
    }
}

impl RenderPlan {
    pub fn space_out(&mut self, letter_spacing: i32) {
        if letter_spacing == 0 {
            return;
        }
        if let Some((_, start)) = self.glyphs.split_last_mut() {
            let len = start.len() as i32;
            for glyph in start {
                glyph.advance.x += letter_spacing;
            }
            self.width += len * letter_spacing;
        }
    }

    pub fn index_from_advance(&self, advance: i32) -> usize {
        let mut sum = 0;
        let mut index = 0;
        while index < self.glyphs.len() {
            let gad = self.glyphs[index].advance.x;
            sum += gad;
            if sum > advance {
                if sum - advance < advance - sum + gad {
                    index += 1;
                }
                break;
            }
            index += 1;
        }
        index
    }

    pub fn total_advance(&self, index: usize) -> i32 {
        self.glyphs.iter().take(index).map(|g| g.advance.x).sum()
    }
}

pub struct Font {
    size: u32,
    dpi: u16,
    pub ellipsis: RenderPlan,
    pub x_heights: (u32, u32),
}

impl Font {
    fn new() -> Font {
        Font {
            size: 0,
            dpi: 0,
            ellipsis: RenderPlan::default(),
            x_heights: (0, 0),
        }
    }

    fn scale(&self) -> i32 {
        let px = (self.size as i64 * self.dpi as i64) / (64 * 72);
        (px / 8).max(1) as i32
    }

    fn advance(&self) -> i32 {
        8 * self.scale()
    }

    pub fn set_size(&mut self, size: u32, dpi: u16) {
        if self.size == size && self.dpi == dpi && self.x_heights.0 != 0 {
            return;
        }
        self.size = size;
        self.dpi = dpi;
        let cell = self.advance();
        let height = (8 * self.scale()) as u32;
        self.x_heights = (height, height);
        self.ellipsis = RenderPlan {
            width: cell,
            glyphs: vec![GlyphPlan {
                codepoint: ELLIPSIS,
                advance: pt!(cell, 0),
            }],
        };
    }

    pub fn em(&self) -> u16 {
        self.advance() as u16
    }

    pub fn plan<S: AsRef<str>>(
        &mut self,
        text: S,
        max_width: Option<i32>,
        _features: Option<&[String]>,
    ) -> RenderPlan {
        let cell = self.advance();
        let mut render_plan = RenderPlan::default();
        for ch in text.as_ref().chars() {
            render_plan.glyphs.push(GlyphPlan {
                codepoint: ch as u32,
                advance: pt!(cell, 0),
            });
            render_plan.width += cell;
        }
        if let Some(max_width) = max_width {
            self.crop_right(&mut render_plan, max_width);
        }
        render_plan
    }

    pub fn crop_right(&self, render_plan: &mut RenderPlan, max_width: i32) {
        if render_plan.width <= max_width || self.ellipsis.glyphs.is_empty() {
            return;
        }
        render_plan.width += self.ellipsis.width;
        while let Some(glyph) = render_plan.glyphs.pop() {
            render_plan.width -= glyph.advance.x;
            if render_plan.width <= max_width {
                break;
            }
        }
        render_plan.glyphs.extend_from_slice(&self.ellipsis.glyphs);
    }

    /// Keep the glyphs around `index` that fit in `max_width`.
    /// The returned index is the first original glyph still shown.
    /// A left-hand cut inserts one ellipsis glyph, which the text fields
    /// account for by stepping the cursor one place to the right.
    pub fn crop_around(&self, render_plan: &mut RenderPlan, index: usize, max_width: i32) -> usize {
        if render_plan.width <= max_width || render_plan.glyphs.is_empty() {
            return 0;
        }

        let len = render_plan.glyphs.len();
        let mut width = 0;
        let mut polarity = 0i32;
        let mut upper_index = index.min(len);
        let mut lower_index = index as i32 - 1;

        for _ in 0..(len + 2) {
            if upper_index < len && (polarity % 2 == 0 || lower_index < 0) {
                let next_width = width + render_plan.glyphs[upper_index].advance.x;
                if next_width > max_width {
                    break;
                }
                width = next_width;
                upper_index += 1;
            } else if lower_index >= 0 && (polarity % 2 == 1 || upper_index == len) {
                let next_width = width + render_plan.glyphs[lower_index as usize].advance.x;
                if next_width > max_width {
                    break;
                }
                width = next_width;
                lower_index -= 1;
            } else {
                break;
            }
            polarity += 1;
        }

        if upper_index < len {
            width += self.ellipsis.width;
            if upper_index > 0 {
                upper_index -= 1;
            }
            while width > max_width && upper_index > lower_index.max(0) as usize {
                width -= render_plan.glyphs[upper_index].advance.x;
                if upper_index == 0 {
                    break;
                }
                upper_index -= 1;
            }
            render_plan.glyphs.truncate(upper_index + 1);
            render_plan.glyphs.extend_from_slice(&self.ellipsis.glyphs);
        }

        if lower_index >= 0 {
            width += self.ellipsis.width;
            lower_index += 1;
            while width > max_width && (lower_index as usize) < upper_index {
                width -= render_plan.glyphs[lower_index as usize].advance.x;
                lower_index += 1;
            }
            let kept = render_plan.glyphs[lower_index as usize..].to_vec();
            render_plan.glyphs = self
                .ellipsis
                .glyphs
                .iter()
                .cloned()
                .chain(kept)
                .collect();
        }

        render_plan.width = width;
        if lower_index < 0 {
            0
        } else {
            lower_index as usize
        }
    }

    pub fn render(&mut self, fb: &mut dyn Framebuffer, color: Color, render_plan: &RenderPlan, origin: Point) {
        let scale = self.scale();
        let cell = 8 * scale;
        let width = fb.width() as i32;
        let height = fb.height() as i32;
        let mut x = origin.x;

        for glyph in &render_plan.glyphs {
            let rows = glyph_rows(glyph.codepoint);
            for row in 0..8 {
                let bits = rows[row];
                if bits == 0 {
                    continue;
                }
                for col in 0..8 {
                    if bits & (1 << col) == 0 {
                        continue;
                    }
                    let left = x + (col as i32) * scale;
                    let top = origin.y - cell + (row as i32) * scale;
                    for sy in 0..scale {
                        for sx in 0..scale {
                            let px = left + sx;
                            let py = top + sy;
                            if px >= 0 && py >= 0 && px < width && py < height {
                                fb.set_blended_pixel(px as u32, py as u32, color, 1.0);
                            }
                        }
                    }
                }
            }
            x += glyph.advance.x;
        }
    }
}

fn glyph_rows(codepoint: u32) -> &'static [u8; 8] {
    if codepoint == ELLIPSIS {
        return &ELLIPSIS_ROWS;
    }
    if codepoint < 128 {
        &FONT8X8[codepoint as usize]
    } else {
        &FONT8X8[b'?' as usize]
    }
}

pub struct Fonts {
    sans_serif: Font,
    serif: Font,
    monospace: Font,
    keyboard: Font,
    display: Font,
}

impl Fonts {
    pub fn load() -> Result<Fonts, Error> {
        Ok(Fonts {
            sans_serif: Font::new(),
            serif: Font::new(),
            monospace: Font::new(),
            keyboard: Font::new(),
            display: Font::new(),
        })
    }
}

pub fn font_from_style<'a>(fonts: &'a mut Fonts, style: &Style, dpi: u16) -> &'a mut Font {
    let font = match style.family {
        Family::SansSerif => &mut fonts.sans_serif,
        Family::Serif => &mut fonts.serif,
        Family::Monospace => &mut fonts.monospace,
        Family::Keyboard => &mut fonts.keyboard,
        Family::Display => &mut fonts.display,
    };
    let _ = style.variant;
    font.set_size(style.size, dpi);
    font
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_and_crops_an_address() {
        let mut font = Font::new();
        font.set_size(524, 300);
        let mut plan = font.plan("192.168.1.20", None, None);
        assert_eq!(plan.glyphs.len(), 12);
        let cell = font.advance();
        assert_eq!(plan.width, 12 * cell);
        let lower = font.crop_around(&mut plan, 8, 5 * cell);
        assert!(lower <= 8);
        assert!(!plan.glyphs.is_empty());
    }
}
