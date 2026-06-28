use crate::settings::FontSettings;
use iced::{Font, Size};
use iced_core::{
    alignment::Vertical,
    text::{Alignment, LineHeight, Paragraph, Shaping as TextShaping},
    Text,
};
use iced_graphics::text::paragraph;

#[derive(Debug, Clone)]
pub struct TermFont {
    pub(crate) size: f32,
    pub(crate) font_type: Font,
    pub(crate) scale_factor: f32,
    pub(crate) measure: Size<f32>,
}

impl TermFont {
    pub fn new(settings: FontSettings) -> Self {
        Self {
            size: settings.size,
            font_type: settings.font_type,
            scale_factor: settings.scale_factor,
            measure: font_measure(settings.size, settings.scale_factor, settings.font_type),
        }
    }

    pub fn sync(&mut self) {
        self.measure = font_measure(self.size, self.scale_factor, self.font_type)
    }
}

/// The size of a single monospace cell.
///
/// Width is measured from a *run* of identical glyphs and divided back out, so
/// the result is the font's true per-glyph advance with no single-glyph
/// side-bearing error (measuring a lone `"m"` was a source of the squish). Height
/// is the line box (`font_size × line_height`), the exact value the renderer steps
/// rows by, so rows tile without gaps or overlap.
fn font_measure(font_size: f32, scale_factor: f32, font_type: Font) -> Size<f32> {
    const SAMPLE: &str = "MMMMMMMMMMMMMMMMMMMM"; // 20 glyphs
    let paragraph = paragraph::Paragraph::with_text(Text {
        content: SAMPLE,
        font: font_type,
        size: iced_core::Pixels(font_size),
        align_y: Vertical::Top,
        align_x: Alignment::Left,
        shaping: TextShaping::Advanced,
        line_height: LineHeight::Relative(scale_factor),
        bounds: Size::INFINITE,
        wrapping: iced_core::text::Wrapping::None,
    });

    let width = paragraph.min_bounds().width / SAMPLE.chars().count() as f32;
    let height = LineHeight::Relative(scale_factor)
        .to_absolute(iced_core::Pixels(font_size))
        .0;
    Size::new(width.max(1.0), height.max(1.0))
}
