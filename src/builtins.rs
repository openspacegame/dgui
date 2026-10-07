//! Convenience constructors returning the same Frame as every other element.
use crate::{Align, Color, Frame, Justify, State};
use std::rc::Rc;

const FONT_SIZE: f32 = 16.0;
fn text_color() -> egui::Color32 {
    egui::Color32::from_rgb(225, 231, 240)
}
fn text_layout(ctx: &egui::Context, text: &str, width: f32) -> std::sync::Arc<egui::Galley> {
    ctx.fonts_mut(|fonts| {
        fonts.layout(
            text.to_owned(),
            egui::FontId::proportional(FONT_SIZE),
            text_color(),
            width.max(0.0),
        )
    })
}
impl<'a> Frame<'a> {
    /// Text content that wraps within this frame's content rectangle.
    pub fn text(text: impl Into<String>) -> Self {
        let text: Rc<str> = text.into().into();
        let measured = text.clone();
        Self::canvas(
            move |ctx, input| text_layout(ctx, &measured, input.width()).size().into(),
            move |canvas| {
                let galley = text_layout(canvas.ui.ctx(), &text, canvas.content_rect.width());
                canvas
                    .ui
                    .painter()
                    .galley(canvas.content_rect.min, galley, text_color());
            },
        )
    }
}

/// A focusable frame containing centered text. All decoration and click handling
/// comes from the common frame machinery. `children` can replace its text.
pub fn button<'a>(text: impl Into<String>) -> Frame<'a> {
    let text = text.into();
    Frame::new()
        .padding(8.0)
        .corner_radius(6)
        .background(Color::rgb(49, 67, 91))
        .hover_background(Color::rgb(61, 84, 114))
        .active_background(Color::rgb(37, 50, 68))
        .focus_background(Color::rgb(61, 84, 114))
        .align(Align::Center)
        .justify(Justify::Center)
        .focusable(true)
        .children(move |ui| ui.add(Frame::text(text)))
}

/// A frame containing a native single-line editor. Its decoration and event
/// handlers are shared with all frames; changes commit after the tree is drawn.
pub fn text_input<'a>(value: State<String>) -> Frame<'a> {
    let mut text = value.get();
    Frame::canvas(
        |ctx, _| [200.0, text_layout(ctx, "M", f32::INFINITY).size().y],
        move |canvas| {
            let response = egui::TextEdit::singleline(&mut text)
                .id(canvas.id)
                .font(egui::FontId::proportional(FONT_SIZE))
                .text_color(text_color())
                .frame(egui::Frame::NONE)
                .clip_text(true)
                .desired_width(canvas.content_rect.width())
                .show(canvas.ui)
                .response;
            let changed = response.changed();
            canvas.respond((*response).clone());
            if changed {
                canvas.defer(move || value.set(text));
            }
        },
    )
    .padding(8.0)
    .corner_radius(6)
    .background(Color::rgb(17, 25, 38))
    .clickable(true)
}
