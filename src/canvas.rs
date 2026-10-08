//! Measurement and drawing access for canvas frames.
use crate::Callback;

/// Space available for content along one axis during layout.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AvailableSpace {
    /// A fixed amount of space, in points.
    Definite(f32),
    /// Layout is asking for the content's smallest size, e.g. text wrapped
    /// at every opportunity.
    MinContent,
    /// Layout is asking for the content's largest useful size, e.g. text
    /// without wrapping.
    MaxContent,
}

/// The constraints passed to a [`Frame::canvas`](crate::Frame::canvas)
/// measure function. Sizes exclude the frame's padding and border.
///
/// Layout may measure the same canvas several times with different inputs.
#[derive(Clone, Copy, Debug)]
pub struct MeasureInput {
    /// `[width, height]` already fixed by layout, if any. dgui uses these
    /// in place of the measured size, so a measure function may ignore them,
    /// but should use a known width when computing the height of wrapped
    /// content.
    pub known: [Option<f32>; 2],
    /// `[width, height]` space available to the content.
    pub available: [AvailableSpace; 2],
}
impl MeasureInput {
    /// The width to wrap content at: the known width if there is one,
    /// otherwise the available width, with `MinContent` as 0 and `MaxContent`
    /// as infinity.
    pub fn width(self) -> f32 {
        self.known[0].unwrap_or(match self.available[0] {
            AvailableSpace::Definite(width) => width,
            AvailableSpace::MinContent => 0.0,
            AvailableSpace::MaxContent => f32::INFINITY,
        })
    }
}

/// What a [`Frame::canvas`](crate::Frame::canvas) draw function receives:
/// the frame's final geometry, its interaction, and a way to defer effects.
pub struct Canvas<'a, 'frame> {
    /// An egui `Ui` constrained and clipped to [`content_rect`](Self::content_rect).
    pub ui: &'a mut egui::Ui,
    /// An id that is stable across frames for this frame. Use it for the
    /// native egui widget this canvas reports with [`respond`](Self::respond).
    pub id: egui::Id,
    /// The frame's full rectangle, including padding and border. The frame's
    /// event handlers cover this whole area.
    pub frame_rect: egui::Rect,
    /// The rectangle inside padding and border, where content goes.
    pub content_rect: egui::Rect,
    pub(crate) response: egui::Response,
    pub(crate) content_response: Option<egui::Response>,
    pub(crate) frame_clip: egui::Rect,
    pub(crate) callbacks: &'a mut Vec<Callback<'frame>>,
}
impl<'frame> Canvas<'_, 'frame> {
    /// Runs `callback` after the entire tree has been drawn, alongside event
    /// callbacks. Use this for writes to application state, such as
    /// committing an edit, so the tree doesn't change while it is drawn.
    pub fn defer(&mut self, callback: impl FnOnce() + 'frame) {
        self.callbacks.push(Box::new(callback));
    }
    /// The frame's own egui response, covering [`frame_rect`](Self::frame_rect).
    /// What it senses is configured with [`Frame`](crate::Frame)'s event
    /// methods.
    pub fn response(&self) -> &egui::Response {
        &self.response
    }

    /// Reports the response of a native egui control drawn in this canvas,
    /// such as a `TextEdit`, so that the frame's event handlers see its clicks,
    /// hover, and focus too. Clicks in the frame's padding then focus the
    /// control.
    ///
    /// Give the control [`id`](Self::id) as its id. Report at most one
    /// control per canvas; a later call replaces an earlier one.
    pub fn respond(&mut self, response: egui::Response) {
        if self.response.clicked() {
            response.request_focus();
        }
        self.content_response = Some(response);
    }
    /// A painter clipped to the whole frame (within its ancestors' clipping)
    /// rather than to the content rectangle, for decorations that cover the
    /// padding.
    pub fn frame_painter(&self) -> egui::Painter {
        let mut painter = self.ui.painter().clone();
        painter.set_clip_rect(self.frame_clip);
        painter
    }
}

type Measure<'a> = Box<dyn Fn(&egui::Context, MeasureInput) -> [f32; 2] + 'a>;
type Paint<'a> = Box<dyn FnOnce(&mut Canvas<'_, 'a>) + 'a>;
pub(crate) struct CanvasContent<'a> {
    pub measure: Measure<'a>,
    pub paint: Option<Paint<'a>>,
}
