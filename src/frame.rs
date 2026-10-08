use crate::canvas::CanvasContent;
use crate::{
    Align, Callback, Canvas, Color, Direction, Justify, Length, MeasureInput, Overflow, Style, Ui,
};

type Children<'a> = Box<dyn FnOnce(&mut Ui<'_, 'a>) + 'a>;

/// The single element type: containers, text, editors, buttons, and canvases
/// all have the same styling and event methods. Children build when added to Ui.
#[derive(Default)]
pub struct Frame<'a> {
    pub(crate) style: Style,
    pub(crate) canvas: Option<CanvasContent<'a>>,
    pub(crate) children: Option<Children<'a>>,
    pub(crate) events: Events<'a>,
    pub(crate) clickable: bool,
    pub(crate) focusable: bool,
    pub(crate) disabled: bool,
    pub(crate) sense: Option<egui::Sense>,
    pub(crate) accessibility_label: Option<String>,
}
#[derive(Default)]
pub(crate) struct Events<'a> {
    pub click: Option<Callback<'a>>,
    pub hover: Option<Callback<'a>>,
    pub focus: Option<Callback<'a>>,
    pub blur: Option<Callback<'a>>,
    pub response: Option<Box<dyn FnOnce(egui::Response) + 'a>>,
}

macro_rules! setters {
    ($($name:ident: $ty:ty),* $(,)?) => {$ (
        pub fn $name(mut self, value: $ty) -> Self { self.style = self.style.$name(value); self }
    )*};
}
macro_rules! lengths {
    ($($name:ident),* $(,)?) => {$ (
        pub fn $name(mut self, value: impl Into<Length>) -> Self { self.style = self.style.$name(value); self }
    )*};
}
impl<'a> Frame<'a> {
    pub fn new() -> Self {
        Self::default()
    }
    /// Replace the frame's contents with children built in the current state scope.
    /// Use child text/canvas frames when combining different kinds of content.
    pub fn children(mut self, children: impl FnOnce(&mut Ui<'_, 'a>) + 'a) -> Self {
        self.canvas = None;
        self.children = Some(Box::new(children));
        self
    }
    /// A frame with measured custom content. Measurement must be side-effect free.
    pub fn canvas(
        measure: impl Fn(&egui::Context, MeasureInput) -> [f32; 2] + 'a,
        draw: impl FnOnce(&mut Canvas<'_, 'a>) + 'a,
    ) -> Self {
        Self {
            canvas: Some(CanvasContent {
                measure: Box::new(measure),
                paint: Some(Box::new(draw)),
            }),
            ..Self::default()
        }
    }
    /// A canvas with a fixed preferred content size and ordinary egui access.
    pub fn egui_canvas(size: [f32; 2], draw: impl FnOnce(&mut egui::Ui) + 'a) -> Self {
        Self::canvas(move |_, _| size, move |canvas| draw(canvas.ui))
    }
    /// Replaces the complete style; individual setters preserve other properties.
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }
    setters! { direction: Direction, wrap: bool, gap: f32, padding: f32, margin: f32,
    grow: f32, shrink: f32, align: Align, justify: Justify, corner_radius: u8,
    overflow_x: Overflow, overflow_y: Overflow,
    background: Color, hover_background: Color, active_background: Color, focus_background: Color }
    lengths! { width, height, min_width, min_height, max_width, max_height }
    pub fn border(mut self, width: f32, color: Color) -> Self {
        self.style = self.style.border(width, color);
        self
    }
    /// Respond to pointer clicks, without bubbling to ancestor frames.
    pub fn on_click(mut self, callback: impl FnOnce() + 'a) -> Self {
        self.events.click = Some(Box::new(callback));
        self
    }
    /// Runs once per render cycle while this frame is hovered.
    pub fn on_hover(mut self, callback: impl FnOnce() + 'a) -> Self {
        self.events.hover = Some(Box::new(callback));
        self
    }
    pub fn on_focus(mut self, callback: impl FnOnce() + 'a) -> Self {
        self.events.focus = Some(Box::new(callback));
        self
    }
    pub fn on_blur(mut self, callback: impl FnOnce() + 'a) -> Self {
        self.events.blur = Some(Box::new(callback));
        self
    }
    /// Make a pointer target even without a click handler, e.g. an editor's padding.
    pub fn clickable(mut self, value: bool) -> Self {
        self.clickable = value;
        self
    }
    /// Enable tab focus and Enter/Space activation for the frame itself.
    /// Native canvas controls such as text editors manage their own focus.
    pub fn focusable(mut self, value: bool) -> Self {
        self.focusable = value;
        self
    }
    /// Disable interaction for this frame and its descendants.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.disabled = !enabled;
        self
    }
    /// Additional pointer interaction, such as dragging, for this frame.
    pub fn sense(mut self, sense: egui::Sense) -> Self {
        self.sense = Some(sense);
        self
    }
    /// Describe a composed control to accessibility tools.
    pub fn accessibility_label(mut self, label: impl Into<String>) -> Self {
        self.accessibility_label = Some(label.into());
        self
    }
    /// Observe the combined frame/native response after the tree is drawn.
    /// Runs each frame, including for geometry, hover, and held input.
    pub fn on_response(mut self, callback: impl FnOnce(egui::Response) + 'a) -> Self {
        self.events.response = Some(Box::new(callback));
        self
    }
}
