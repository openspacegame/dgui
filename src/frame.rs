use crate::canvas::CanvasContent;
use crate::{
    Align, Callback, Canvas, Color, Direction, Justify, Length, MeasureInput, Overflow, Style, Ui,
};

type Children<'a> = Box<dyn FnOnce(&mut Ui<'_, 'a>) + 'a>;

/// The single element type of a dgui tree: a flexbox box with optional
/// children or custom content, decoration, and event handlers.
///
/// Containers, [text](Frame::text), [buttons](crate::button),
/// [text inputs](crate::text_input), [canvases](Frame::canvas) and
/// [virtual lists](Frame::virtual_list) are all `Frame`s, so every styling and
/// event method works on all of them. A frame does nothing until it is passed
/// to [`Ui::add`].
///
/// The style setters (`width`, `padding`, `background`, …) set the matching
/// field of [`Style`]; see there for what each one means and its default.
///
/// ```
/// use dgui::{Color, Direction, Frame, Justify, button};
///
/// # let _: Frame<'_> =
/// Frame::new()
///     .direction(Direction::Row)
///     .justify(Justify::SpaceBetween)
///     .padding(12.0)
///     .background(Color::rgb(28, 38, 55))
///     .children(|ui| {
///         ui.add(Frame::text("Title"));
///         ui.add(button("×").on_click(|| println!("closed")));
///     });
/// ```
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
        #[doc = concat!("Sets [`Style::", stringify!($name), "`].")]
        pub fn $name(mut self, value: $ty) -> Self { self.style = self.style.$name(value); self }
    )*};
}
macro_rules! lengths {
    ($($name:ident),* $(,)?) => {$ (
        #[doc = concat!("Sets [`Style::", stringify!($name), "`]. Accepts an `f32` in points or a [`Length`].")]
        pub fn $name(mut self, value: impl Into<Length>) -> Self { self.style = self.style.$name(value); self }
    )*};
}
impl<'a> Frame<'a> {
    /// An empty frame with the default [`Style`]: an auto-sized column.
    pub fn new() -> Self {
        Self::default()
    }
    /// Sets the closure that builds this frame's children.
    ///
    /// The closure runs when the frame is passed to [`Ui::add`], in the
    /// current state scope. This replaces any canvas content (such as text);
    /// to combine text with other content, add a [`Frame::text`] child.
    pub fn children(mut self, children: impl FnOnce(&mut Ui<'_, 'a>) + 'a) -> Self {
        self.canvas = None;
        self.children = Some(Box::new(children));
        self
    }
    /// A leaf frame with custom content that you measure and paint.
    ///
    /// - `measure` returns the content's preferred `[width, height]`, excluding
    ///   padding and border. Layout may call it many times with different
    ///   [`MeasureInput`]s, so it must be cheap and free of side effects.
    /// - `draw` runs once, after layout, with the final geometry in a
    ///   [`Canvas`]. Use [`Canvas::defer`] for effects on application state.
    ///
    /// ```
    /// use dgui::Frame;
    ///
    /// // A bar as wide as it is allowed to be, 8 points tall.
    /// # let _: Frame<'_> =
    /// Frame::canvas(
    ///     |_, input| [input.width(), 8.0],
    ///     |canvas| {
    ///         let painter = canvas.ui.painter();
    ///         painter.rect_filled(canvas.content_rect, 4.0, egui::Color32::LIGHT_BLUE);
    ///     },
    /// );
    /// ```
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
    /// A canvas with a fixed preferred content size whose `draw` receives a
    /// plain egui [`Ui`](egui::Ui), constrained and clipped to the content
    /// rectangle.
    ///
    /// Layout may still stretch or shrink the frame; read the final size from
    /// `ui.max_rect()`.
    pub fn egui_canvas(size: [f32; 2], draw: impl FnOnce(&mut egui::Ui) + 'a) -> Self {
        Self::canvas(move |_, _| size, move |canvas| draw(canvas.ui))
    }
    /// Replaces the complete style. Setters called afterwards modify it.
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }
    setters! { direction: Direction, wrap: bool, gap: f32, padding: f32, margin: f32,
    grow: f32, shrink: f32, align: Align, justify: Justify, corner_radius: u8,
    overflow_x: Overflow, overflow_y: Overflow,
    background: Color, hover_background: Color, active_background: Color, focus_background: Color }
    lengths! { width, height, min_width, min_height, max_width, max_height }
    /// Draws a border of `width` points inside the frame's edges. The border
    /// takes up layout space, like padding.
    pub fn border(mut self, width: f32, color: Color) -> Self {
        self.style = self.style.border(width, color);
        self
    }
    /// Runs `callback` after drawing if this frame was clicked, or activated
    /// with Enter/Space while focused.
    ///
    /// Clicks go to the topmost frame under the pointer and don't bubble to
    /// its ancestors. Not called while the frame is disabled.
    pub fn on_click(mut self, callback: impl FnOnce() + 'a) -> Self {
        self.events.click = Some(Box::new(callback));
        self
    }
    /// Runs `callback` after drawing, once per frame, while the pointer is over
    /// this frame.
    pub fn on_hover(mut self, callback: impl FnOnce() + 'a) -> Self {
        self.events.hover = Some(Box::new(callback));
        self
    }
    /// Runs `callback` after drawing in the frame where this frame, or the
    /// native control reported by its canvas, gains keyboard focus.
    pub fn on_focus(mut self, callback: impl FnOnce() + 'a) -> Self {
        self.events.focus = Some(Box::new(callback));
        self
    }
    /// Runs `callback` after drawing in the frame where this frame, or the
    /// native control reported by its canvas, loses keyboard focus.
    pub fn on_blur(mut self, callback: impl FnOnce() + 'a) -> Self {
        self.events.blur = Some(Box::new(callback));
        self
    }
    /// Makes the frame a click target even without an
    /// [`on_click`](Self::on_click) handler, so it receives clicks instead of
    /// whatever is beneath it. [`text_input`](crate::text_input) uses this so
    /// clicks in its padding focus the editor.
    pub fn clickable(mut self, value: bool) -> Self {
        self.clickable = value;
        self
    }
    /// Lets the frame itself take keyboard focus with Tab and be activated with
    /// Enter/Space. While focused, it shows [`Style::focus_background`].
    ///
    /// Not needed for native controls inside a canvas, such as the editor in
    /// [`text_input`](crate::text_input), which manage their own focus.
    pub fn focusable(mut self, value: bool) -> Self {
        self.focusable = value;
        self
    }
    /// Enables or disables interaction for this frame and its descendants.
    /// Disabled frames don't fire event callbacks and are drawn by egui as
    /// disabled. Frames are enabled by default.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.disabled = !enabled;
        self
    }
    /// Senses additional pointer interaction, such as dragging. Read the
    /// result in [`on_response`](Self::on_response).
    pub fn sense(mut self, sense: egui::Sense) -> Self {
        self.sense = Some(sense);
        self
    }
    /// Describes the frame to screen readers and other accessibility tools.
    /// Use it for controls composed from several frames, like an icon button.
    pub fn accessibility_label(mut self, label: impl Into<String>) -> Self {
        self.accessibility_label = Some(label.into());
        self
    }
    /// Runs `callback` after drawing with this frame's egui [`Response`](egui::Response),
    /// combined with the response of any native control its canvas reported.
    ///
    /// Unlike the other event methods, this runs every frame, so it can read
    /// geometry, drags, held buttons, and anything else egui reports.
    pub fn on_response(mut self, callback: impl FnOnce(egui::Response) + 'a) -> Self {
        self.events.response = Some(Box::new(callback));
        self
    }
}
