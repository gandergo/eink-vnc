mod freetype_sys;

use self::freetype_sys::*;
use std::ptr;
use std::rc::Rc;
use anyhow::{Error, format_err};
use crate::color::Color;
use crate::framebuffer::Framebuffer;
use crate::geom::Point;

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

// DejaVu Sans 2.37 (Bitstream Vera license, DejaVu changes in the public domain).
// Embedded so the Kobo binary can draw the connection form without a fonts directory.
// The files sit next to this module. See LICENSE in this directory.
// Each face is opened separately so set_size on one family does not change another.
const SANS_REGULAR: &[u8] = include_bytes!("DejaVuSans.ttf");
const SANS_ITALIC: &[u8] = include_bytes!("DejaVuSans-Oblique.ttf");
const SANS_BOLD: &[u8] = include_bytes!("DejaVuSans-Bold.ttf");
const SANS_BOLD_ITALIC: &[u8] = include_bytes!("DejaVuSans-BoldOblique.ttf");

#[derive(Debug, Copy, Clone)]
struct GlyphPlan {
    glyph_id: u32,
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

struct FontLibrary(*mut FtLibrary);

pub struct Font {
    // Keeps the FreeType library alive until every face has been closed.
    #[allow(dead_code)]
    lib: Rc<FontLibrary>,
    face: *mut FtFace,
    size: u32,
    dpi: u16,
    pub ellipsis: RenderPlan,
    pub x_heights: (u32, u32),
}

impl FontLibrary {
    fn new() -> Result<Rc<FontLibrary>, Error> {
        unsafe {
            let mut lib = ptr::null_mut();
            let ret = FT_Init_FreeType(&mut lib);
            if ret != FT_ERR_OK {
                Err(format_err!("FreeType failed to start ({ret})"))
            } else {
                Ok(Rc::new(FontLibrary(lib)))
            }
        }
    }
}

impl Drop for FontLibrary {
    fn drop(&mut self) {
        unsafe {
            if !self.0.is_null() {
                FT_Done_FreeType(self.0);
            }
        }
    }
}

impl Drop for Font {
    fn drop(&mut self) {
        unsafe {
            if !self.face.is_null() {
                FT_Done_Face(self.face);
            }
        }
    }
}

fn open_static(lib: &Rc<FontLibrary>, bytes: &'static [u8]) -> Result<Font, Error> {
    unsafe {
        let mut face = ptr::null_mut();
        let ret = FT_New_Memory_Face(
            lib.0,
            bytes.as_ptr(),
            bytes.len() as libc::c_long,
            0,
            &mut face,
        );
        if ret != FT_ERR_OK {
            return Err(format_err!("FreeType cannot open the embedded font ({ret})"));
        }
        Ok(Font {
            lib: Rc::clone(lib),
            face,
            size: 0,
            dpi: 0,
            ellipsis: RenderPlan::default(),
            x_heights: (0, 0),
        })
    }
}

impl Font {
    pub fn set_size(&mut self, size: u32, dpi: u16) {
        if self.size == size && self.dpi == dpi && self.x_heights.0 != 0 {
            return;
        }
        self.size = size;
        self.dpi = dpi;
        unsafe {
            let ret = FT_Set_Char_Size(self.face, size as FtF26Dot6, 0, dpi as libc::c_uint, 0);
            if ret != FT_ERR_OK {
                self.x_heights = (0, 0);
                return;
            }
        }
        self.x_heights = (self.pixel_height('x'), self.pixel_height('X'));
        // One character, so a left-hand crop still inserts exactly one ellipsis glyph.
        self.ellipsis = self.plan("\u{2026}", None, None);
    }

    pub fn em(&self) -> u16 {
        unsafe { (*(*self.face).size).metrics.x_ppem as u16 }
    }

    /// Lay the text out one FreeType glyph per Unicode scalar.
    /// `features` is ignored: HarfBuzz is not linked, so ligatures are not formed.
    pub fn plan<S: AsRef<str>>(
        &mut self,
        text: S,
        max_width: Option<i32>,
        _features: Option<&[String]>,
    ) -> RenderPlan {
        let mut render_plan = RenderPlan::default();
        let mut previous = 0u32;
        for ch in text.as_ref().chars() {
            let glyph_id = unsafe { glyph_id(self.face, ch) };
            let mut advance = self.glyph_advance(glyph_id);
            if previous != 0 {
                advance += self.kerning(previous, glyph_id);
            }
            render_plan.width += advance;
            render_plan.glyphs.push(GlyphPlan {
                glyph_id,
                advance: pt!(advance, 0),
            });
            previous = glyph_id;
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
        let fb_width = fb.width() as i32;
        let fb_height = fb.height() as i32;
        let mut x = origin.x;

        for glyph in &render_plan.glyphs {
            unsafe {
                if FT_Load_Glyph(self.face, glyph.glyph_id, FT_LOAD_RENDER) != FT_ERR_OK {
                    x += glyph.advance.x;
                    continue;
                }
                let slot = &*(*self.face).glyph;
                let bitmap = &slot.bitmap;
                if bitmap.buffer.is_null() || bitmap.rows <= 0 || bitmap.width <= 0 {
                    x += glyph.advance.x;
                    continue;
                }
                let left = x + slot.bitmap_left;
                let top = origin.y - slot.bitmap_top;
                for row in 0..bitmap.rows {
                    let row_ptr = bitmap.buffer.offset((row * bitmap.pitch) as isize);
                    for col in 0..bitmap.width {
                        let px = left + col;
                        let py = top + row;
                        if px < 0 || py < 0 || px >= fb_width || py >= fb_height {
                            continue;
                        }
                        let blackness = *row_ptr.add(col as usize);
                        if blackness == 0 {
                            continue;
                        }
                        let alpha = blackness as f32 / 255.0;
                        fb.set_blended_pixel(px as u32, py as u32, color, alpha);
                    }
                }
            }
            x += glyph.advance.x;
        }
    }

    fn glyph_advance(&self, glyph_id: u32) -> i32 {
        unsafe {
            if FT_Load_Glyph(self.face, glyph_id, FT_LOAD_DEFAULT) != FT_ERR_OK {
                return 0;
            }
            ((*(*self.face).glyph).advance.x >> 6) as i32
        }
    }

    fn kerning(&self, left: u32, right: u32) -> i32 {
        unsafe {
            let mut delta = FtVector { x: 0, y: 0 };
            if FT_Get_Kerning(self.face, left, right, FT_KERNING_DEFAULT, &mut delta) != FT_ERR_OK {
                return 0;
            }
            (delta.x >> 6) as i32
        }
    }

    fn pixel_height(&self, ch: char) -> u32 {
        unsafe {
            let id = glyph_id(self.face, ch);
            if FT_Load_Glyph(self.face, id, FT_LOAD_DEFAULT) != FT_ERR_OK {
                return 0;
            }
            ((*(*self.face).glyph).metrics.height >> 6) as u32
        }
    }
}

unsafe fn glyph_id(face: *mut FtFace, ch: char) -> u32 {
    let id = FT_Get_Char_Index(face, ch as libc::c_ulong);
    if id != 0 {
        return id;
    }
    let question = FT_Get_Char_Index(face, '?' as libc::c_ulong);
    if question != 0 {
        question
    } else {
        0
    }
}

struct FontFamily {
    regular: Font,
    italic: Font,
    bold: Font,
    bold_italic: Font,
}

pub struct Fonts {
    sans_serif: FontFamily,
    serif: FontFamily,
    monospace: FontFamily,
    keyboard: Font,
    display: Font,
}

fn open_family(lib: &Rc<FontLibrary>) -> Result<FontFamily, Error> {
    Ok(FontFamily {
        regular: open_static(lib, SANS_REGULAR)?,
        italic: open_static(lib, SANS_ITALIC)?,
        bold: open_static(lib, SANS_BOLD)?,
        bold_italic: open_static(lib, SANS_BOLD_ITALIC)?,
    })
}

impl Fonts {
    pub fn load() -> Result<Fonts, Error> {
        let lib = FontLibrary::new()?;
        Ok(Fonts {
            sans_serif: open_family(&lib)?,
            serif: open_family(&lib)?,
            monospace: open_family(&lib)?,
            keyboard: open_static(&lib, SANS_REGULAR)?,
            display: open_static(&lib, SANS_REGULAR)?,
        })
    }
}

fn font_from_variant(family: &mut FontFamily, variant: Variant) -> &mut Font {
    match variant {
        Variant::BoldItalic => &mut family.bold_italic,
        Variant::Italic => &mut family.italic,
        Variant::Bold => &mut family.bold,
        Variant::Regular => &mut family.regular,
    }
}

pub fn font_from_style<'a>(fonts: &'a mut Fonts, style: &Style, dpi: u16) -> &'a mut Font {
    let font = match style.family {
        Family::SansSerif => font_from_variant(&mut fonts.sans_serif, style.variant),
        Family::Serif => font_from_variant(&mut fonts.serif, style.variant),
        Family::Monospace => font_from_variant(&mut fonts.monospace, style.variant),
        Family::Keyboard => &mut fonts.keyboard,
        Family::Display => &mut fonts.display,
    };
    font.set_size(style.size, dpi);
    font
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Color;
    use crate::framebuffer::{Framebuffer, Pixmap};

    #[test]
    fn plans_one_glyph_per_character_and_draws_an_outline() {
        let mut fonts = Fonts::load().expect("freetype");
        let font = font_from_style(&mut fonts, &NORMAL_STYLE, 300);
        assert_eq!(font.ellipsis.glyphs.len(), 1);
        assert!(font.x_heights.0 > 0);

        let mut plan = font.plan("192.168.1.20", None, None);
        assert_eq!(plan.glyphs.len(), 12);
        let full_width = plan.width;
        assert!(full_width > 0);
        let narrow = font.plan("i", None, None).width;
        let wide = font.plan("M", None, None).width;
        assert!(wide > narrow, "proportional outline advances, i={narrow} M={wide}");

        let lower = font.crop_around(&mut plan, 8, full_width / 3);
        assert!(lower <= 8);
        assert!(!plan.glyphs.is_empty());

        let missing = font.plan("\u{1F600}", None, None);
        assert_eq!(missing.glyphs.len(), 1);

        let letter = font.plan("A", None, None);
        let mut fb = Pixmap::new(96, 96, 1);
        font.render(&mut fb, Color::Gray(0), &letter, pt!(8, 72));
        assert!(
            fb.data.iter().any(|&px| px < 250),
            "the outline of A should darken the pixmap"
        );
    }
}
