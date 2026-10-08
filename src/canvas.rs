//! Measurement and drawing access for canvas frames.
use crate::Callback;

/// Space available for content along one axis during layout.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AvailableSpace {
    Definite(f32),
    MinContent,
    MaxContent,
}

/// Content constraints, excluding frame padding. Measurement may run repeatedly.
/// Known dimensions are authoritative and are enforced by the runtime.
#[derive(Clone, Copy, Debug)]
pub struct MeasureInput {
    pub known: [Option<f32>; 2],
    pub available: [AvailableSpace; 2],
}
impl MeasureInput {
    /// A wrapping width for text measurement.
    pub fn width(self) -> f32 {
        self.known[0].unwrap_or(match self.available[0] {
            AvailableSpace::Definite(width) => width,
            AvailableSpace::MinContent => 0.0,
            AvailableSpace::MaxContent => f32::INFINITY,
        })
    }
}

/// Access to a canvas's final geometry, interaction, and deferred effects.
/// `ui` is constrained and clipped to the content rectangle. Frame event handlers cover the full frame, including padding and border.
pub struct Canvas<'a, 'frame> {
    pub ui: &'a mut egui::Ui,
    pub id: egui::Id,
    pub frame_rect: egui::Rect,
    pub content_rect: egui::Rect,
    pub(crate) response: egui::Response,
    pub(crate) content_response: Option<egui::Response>,
    pub(crate) frame_clip: egui::Rect,
    pub(crate) callbacks: &'a mut Vec<Callback<'frame>>,
}
impl<'frame> Canvas<'_, 'frame> {
    /// Queue application effects until the entire tree has been drawn.
    pub fn defer(&mut self, callback: impl FnOnce() + 'frame) {
        self.callbacks.push(Box::new(callback));
    }
    /// The common frame interaction. Configure it with Frame's event methods.
    pub fn response(&self) -> &egui::Response {
        &self.response
    }

    /// Report native content interaction (e.g. TextEdit) to this frame's handlers.
    /// Use `id` for the native widget. Its clicks, hover, and focus are combined
    /// with the frame's padding interaction; clicks in padding focus the content.
    /// A canvas should report at most one primary native control.
    pub fn respond(&mut self, response: egui::Response) {
        if self.response.clicked() {
            response.request_focus();
        }
        self.content_response = Some(response);
    }
    /// Paint frame decorations, including padding, within ancestor clipping.
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
