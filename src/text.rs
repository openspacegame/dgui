//! Text measurement and drawing share whitespace and line-breaking policies.
use crate::{AvailableSpace, Frame, MeasureInput, Overflow};
use egui::{Color32, FontFamily, FontId, FontSelection, TextStyle};
use std::sync::Arc;
use unicode_segmentation::UnicodeSegmentation;

/// Whitespace preservation and wrapping, following the named CSS modes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WhiteSpace {
    /// Collapse spaces and newlines, and wrap at normal line breaks.
    #[default]
    Normal,
    /// Collapse spaces and newlines without wrapping.
    NoWrap,
    /// Preserve spaces and newlines without wrapping.
    Pre,
    /// Preserve spaces and newlines, allowing wrapping.
    PreWrap,
    /// Preserve newlines, collapse spaces, and allow wrapping.
    PreLine,
}

/// Additional break opportunities for otherwise unbreakable text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OverflowWrap {
    /// Use Unicode line-break opportunities; long words may overflow.
    #[default]
    Normal,
    /// Allow breaks between grapheme clusters, including for intrinsic sizing.
    Anywhere,
}

/// How a clipped single line indicates omitted text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextOverflow {
    /// Clip without an indicator.
    #[default]
    Clip,
    /// Replace the end of an overflowing single line with an ellipsis.
    Ellipsis,
}

/// Horizontal placement of text within its allocated content width.
/// dgui currently supports horizontal, left-to-right layout.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextAlign {
    /// Align to the left edge.
    #[default]
    Start,
    /// Center each line.
    Center,
    /// Align to the right edge.
    End,
}

/// A text leaf whose measurement and painting use the same policies.
///
/// Typography is explicit, not inherited from parent frames. Semantic egui
/// text styles are resolved against the current theme on each layout pass.
/// Convert with [`frame`](Self::frame) to apply ordinary frame layout and events.
#[derive(Clone)]
pub struct Text {
    value: String,
    font: FontSelection,
    family: Option<FontFamily>,
    color: Option<Color32>,
    whitespace: WhiteSpace,
    wrapping: OverflowWrap,
    overflow: TextOverflow,
    align: TextAlign,
}

impl Text {
    /// Body text with normal whitespace and wrapping.
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            font: FontSelection::Style(TextStyle::Body),
            family: None,
            color: None,
            whitespace: WhiteSpace::Normal,
            wrapping: OverflowWrap::Normal,
            overflow: TextOverflow::Clip,
            align: TextAlign::Start,
        }
    }

    /// Select a semantic text style or an explicit font.
    pub fn font(mut self, font: impl Into<FontSelection>) -> Self {
        self.font = font.into();
        self
    }

    /// Override the family while preserving the semantic style's size.
    pub fn font_family(mut self, family: FontFamily) -> Self {
        self.family = Some(family);
        self
    }

    /// Set the text color. The theme's text color is used by default.
    pub fn color(mut self, color: Color32) -> Self {
        self.color = Some(color);
        self
    }

    /// Choose whitespace preservation and whether lines may wrap.
    pub fn white_space(mut self, value: WhiteSpace) -> Self {
        self.whitespace = value;
        self
    }

    /// Choose normal or additional grapheme break opportunities.
    pub fn overflow_wrap(mut self, value: OverflowWrap) -> Self {
        self.wrapping = value;
        self
    }

    /// Choose the overflow indicator. Ellipsis applies to a non-wrapping,
    /// horizontally clipped frame; it does not change intrinsic measurements.
    pub fn text_overflow(mut self, value: TextOverflow) -> Self {
        self.overflow = value;
        self
    }

    /// Align each line within the allocated width.
    pub fn text_align(mut self, value: TextAlign) -> Self {
        self.align = value;
        self
    }

    /// Compose this leaf using the ordinary frame layout and event API.
    pub fn frame<'a>(self) -> Frame<'a> {
        Frame {
            content: crate::Content::Text(self),
            ..Frame::new()
        }
    }

    fn wraps(&self) -> bool {
        matches!(
            self.whitespace,
            WhiteSpace::Normal | WhiteSpace::PreWrap | WhiteSpace::PreLine
        )
    }

    fn preserves_spaces(&self) -> bool {
        matches!(self.whitespace, WhiteSpace::Pre | WhiteSpace::PreWrap)
    }

    fn normalized(&self) -> String {
        let text = self.value.replace("\r\n", "\n").replace('\r', "\n");
        if self.preserves_spaces() {
            return text;
        }
        let mut result = String::with_capacity(text.len());
        let mut space = false;
        for ch in text.chars() {
            if ch == '\n' && self.whitespace == WhiteSpace::PreLine {
                result.push('\n');
                space = false;
            } else if matches!(ch, ' ' | '\t' | '\n' | '\u{000c}') {
                space = true;
            } else {
                if space && !result.is_empty() && !result.ends_with('\n') {
                    result.push(' ');
                }
                result.push(ch);
                space = false;
            }
        }
        result
    }

    fn font_id(&self, ctx: &egui::Context) -> FontId {
        let mut font = self.font.clone().resolve(&ctx.global_style());
        if let Some(family) = &self.family {
            font.family = family.clone();
        }
        font
    }

    fn tint(&self, ctx: &egui::Context) -> Color32 {
        self.color
            .unwrap_or_else(|| ctx.global_style().visuals.text_color())
    }

    fn unwrapped(&self, ctx: &egui::Context, text: &str) -> Arc<egui::Galley> {
        let font = self.font_id(ctx);
        let color = self.tint(ctx);
        ctx.fonts_mut(|fonts| fonts.layout_no_wrap(text.to_owned(), font, color))
    }

    fn pieces<'a>(&self, paragraph: &'a str) -> Vec<&'a str> {
        let mut start = 0;
        unicode_linebreak::linebreaks(paragraph)
            .map(|(end, _)| {
                let piece = &paragraph[start..end];
                start = end;
                piece
            })
            .collect()
    }

    fn line_end<'a>(&self, line: &'a str) -> &'a str {
        if self.preserves_spaces() {
            line
        } else {
            line.trim_end_matches([' ', '\t'])
        }
    }

    fn min_width(&self, ctx: &egui::Context, normalized: &str) -> f32 {
        normalized
            .split('\n')
            .map(|paragraph| {
                if !self.wraps() {
                    return self.unwrapped(ctx, paragraph).size().x;
                }
                let pieces = if self.wrapping == OverflowWrap::Anywhere {
                    paragraph.graphemes(true).collect()
                } else {
                    self.pieces(paragraph)
                };
                pieces
                    .into_iter()
                    .map(|piece| self.unwrapped(ctx, self.line_end(piece)).size().x)
                    .fold(0.0, f32::max)
            })
            .fold(0.0, f32::max)
    }

    fn wrapped(&self, ctx: &egui::Context, text: &str, width: f32) -> String {
        if !self.wraps() || !width.is_finite() || self.unwrapped(ctx, text).size().x <= width {
            return text.to_owned();
        }
        let mut lines = Vec::new();
        for paragraph in text.split('\n') {
            let mut line = String::new();
            for piece in self.pieces(paragraph) {
                let candidate = format!("{line}{piece}");
                if !line.is_empty()
                    && self.unwrapped(ctx, self.line_end(&candidate)).size().x > width
                {
                    lines.push(self.line_end(&line).to_owned());
                    line.clear();
                }
                let piece = if line.is_empty() && !self.preserves_spaces() {
                    piece.trim_start_matches([' ', '\t'])
                } else {
                    piece
                };
                if self.wrapping == OverflowWrap::Anywhere
                    && self.unwrapped(ctx, self.line_end(piece)).size().x > width
                {
                    for grapheme in piece.graphemes(true) {
                        let candidate = format!("{line}{grapheme}");
                        if !line.is_empty()
                            && self.unwrapped(ctx, self.line_end(&candidate)).size().x > width
                        {
                            lines.push(self.line_end(&line).to_owned());
                            line.clear();
                        }
                        line.push_str(grapheme);
                    }
                } else {
                    line.push_str(piece);
                }
            }
            lines.push(self.line_end(&line).to_owned());
        }
        lines.join("\n")
    }

    pub(crate) fn measure(&self, ctx: &egui::Context, input: MeasureInput) -> [f32; 2] {
        let text = self.normalized();
        let width = input.known[0].unwrap_or_else(|| match input.available[0] {
            AvailableSpace::MinContent => self.min_width(ctx, &text),
            AvailableSpace::MaxContent => f32::INFINITY,
            AvailableSpace::Definite(width) => width,
        });
        let wrapped = self.wrapped(ctx, &text, width.max(0.0));
        self.unwrapped(ctx, &wrapped).size().into()
    }

    pub(crate) fn paint(&self, ui: &mut egui::Ui, rect: egui::Rect, overflow: [Overflow; 2]) {
        let ctx = ui.ctx();
        let text = self.normalized();
        let elide = self.overflow == TextOverflow::Ellipsis
            && !self.wraps()
            && overflow[0] != Overflow::Visible;
        let galley = if elide {
            let mut job = egui::text::LayoutJob::simple(
                text,
                self.font_id(ctx),
                self.tint(ctx),
                rect.width(),
            );
            job.halign = match self.align {
                TextAlign::Start => egui::Align::Min,
                TextAlign::Center => egui::Align::Center,
                TextAlign::End => egui::Align::Max,
            };
            job.wrap.max_rows = 1;
            job.wrap.break_anywhere = true;
            ctx.fonts_mut(|fonts| fonts.layout_job(job))
        } else {
            let wrapped = self.wrapped(ctx, &text, rect.width());
            let mut job = egui::text::LayoutJob::simple(
                wrapped,
                self.font_id(ctx),
                self.tint(ctx),
                f32::INFINITY,
            );
            job.halign = match self.align {
                TextAlign::Start => egui::Align::Min,
                TextAlign::Center => egui::Align::Center,
                TextAlign::End => egui::Align::Max,
            };
            ctx.fonts_mut(|fonts| fonts.layout_job(job))
        };
        let x = match self.align {
            TextAlign::Start => rect.left(),
            TextAlign::Center => rect.center().x,
            TextAlign::End => rect.right(),
        };
        ui.painter()
            .galley(egui::pos2(x, rect.top()), galley, self.tint(ctx));
    }

    pub(crate) fn value(&self) -> &str {
        &self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whitespace_modes_preserve_nonbreaking_spaces_and_authored_lines() {
        let text = Text::new("  one \t two\n three\u{a0}four  ");
        assert_eq!(text.normalized(), "one two three\u{a0}four");
        assert_eq!(
            text.clone().white_space(WhiteSpace::PreLine).normalized(),
            "one two\nthree\u{a0}four"
        );
        assert_eq!(
            text.clone().white_space(WhiteSpace::Pre).normalized(),
            text.value
        );
    }

    #[test]
    fn words_define_intrinsic_minimum_and_anywhere_only_breaks_long_words() {
        let ctx = egui::Context::default();
        ctx.run_ui(Default::default(), |_| {
            let text = Text::new("alpha beta");
            let word = text.unwrapped(&ctx, "alpha").size().x;
            assert_eq!(text.min_width(&ctx, &text.normalized()), word);
            assert_eq!(text.wrapped(&ctx, &text.normalized(), word), "alpha\nbeta");
            assert_eq!(text.wrapped(&ctx, "alpha", word / 2.0), "alpha");
            let anywhere = text.clone().overflow_wrap(OverflowWrap::Anywhere);
            assert!(anywhere.min_width(&ctx, &anywhere.normalized()) < word);
            assert_eq!(
                anywhere.wrapped(&ctx, &anywhere.normalized(), word),
                "alpha\nbeta"
            );
            assert!(anywhere.wrapped(&ctx, "alpha", word / 2.0).contains('\n'));
            let nbsp = Text::new("alpha\u{a0}beta");
            assert_eq!(
                nbsp.min_width(&ctx, &nbsp.normalized()),
                nbsp.unwrapped(&ctx, &nbsp.normalized()).size().x
            );
        })
        .drop_without_applying_deltas();
    }

    #[test]
    fn measurement_and_painting_agree_at_fractional_widths_and_zoom() {
        for zoom in [1.0, 1.25, 2.0] {
            let ctx = egui::Context::default();
            ctx.set_pixels_per_point(zoom);
            ctx.options_mut(|o| o.max_passes = 1.try_into().unwrap());
            let measured = std::cell::Cell::new(0.0);
            let output = ctx.run_ui(Default::default(), |ui| {
                let text = Text::new("alpha beta gamma delta");
                measured.set(
                    text.measure(
                        &ctx,
                        MeasureInput {
                            known: [Some(73.25), None],
                            available: [
                                AvailableSpace::Definite(73.25),
                                AvailableSpace::MaxContent,
                            ],
                        },
                    )[1],
                );
                text.paint(
                    ui,
                    egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(73.25, 200.0)),
                    [Overflow::Visible; 2],
                );
            });
            let heights: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some(text.galley.size().y),
                    _ => None,
                })
                .collect();
            assert_eq!(heights, [measured.get()]);
            output.drop_without_applying_deltas();
        }
    }
}
