//! Styling shared by every frame. All lengths are in egui points (logical
//! pixels).

/// A size along one axis. A plain `f32` converts to [`Length::Px`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Length {
    /// Sized by content and flex layout.
    #[default]
    Auto,
    /// A fixed size in points.
    Px(f32),
    /// A fraction of the parent's content size: `Percent(1.0)` fills the
    /// parent and `Percent(0.5)` is half of it.
    Percent(f32),
}

impl From<f32> for Length {
    fn from(value: f32) -> Self {
        Self::Px(value)
    }
}

/// The axis along which a frame lays out its children: the main axis.
/// Like CSS `flex-direction`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Direction {
    /// Left to right.
    Row,
    /// Top to bottom.
    #[default]
    Column,
}

/// How children are placed across the main axis (vertically in a row,
/// horizontally in a column). Like CSS `align-items`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    /// At the top of a row, or the left of a column.
    Start,
    /// Centered.
    Center,
    /// At the bottom of a row, or the right of a column.
    End,
    /// Stretched to fill the cross axis, unless the child has a fixed size.
    #[default]
    Stretch,
}

/// How children and leftover space are distributed along the main axis.
/// Like CSS `justify-content`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Justify {
    /// Packed at the start.
    #[default]
    Start,
    /// Packed in the middle.
    Center,
    /// Packed at the end.
    End,
    /// The first and last child touch the edges, with equal space between
    /// children.
    SpaceBetween,
    /// Equal space on both sides of each child, so the edges get half as much
    /// as the gaps between children.
    SpaceAround,
    /// Equal space between children and at both edges.
    SpaceEvenly,
}

/// What a frame does with content larger than itself, along one axis.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Overflow {
    /// Content is clipped to the frame.
    #[default]
    Hidden,
    /// The frame becomes a scroll area. Content keeps its natural size along
    /// this axis, so the frame needs a bounded size here, from a fixed size,
    /// a max size, or its parent.
    Scroll,
}

/// An sRGB color, as red, green, blue and unmultiplied alpha.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color(pub u8, pub u8, pub u8, pub u8);

impl Color {
    /// An opaque color.
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self(r, g, b, 255)
    }
    pub(crate) fn egui(self) -> egui::Color32 {
        egui::Color32::from_rgba_unmultiplied(self.0, self.1, self.2, self.3)
    }
}

/// Layout and decoration of a [`Frame`](crate::Frame).
///
/// Layout follows CSS flexbox with `box-sizing: border-box`: sizes include
/// padding and border. Unlike CSS, minimum sizes default to zero, so children
/// may shrink below their content size unless given
/// [`shrink(0.0)`](Self::shrink) or a minimum size. Content is clipped to the
/// frame.
///
/// Every field has a builder method of the same name, also available on
/// `Frame`. Lengths accept an `f32` in points or a [`Length`].
///
/// ```
/// use dgui::{Align, Style};
///
/// let toolbar = Style::row().gap(8.0).padding(4.0).align(Align::Center);
/// ```
#[derive(Clone, Debug)]
pub struct Style {
    /// Horizontal overflow behavior. Default: [`Overflow::Hidden`].
    pub overflow_x: Overflow,
    /// Vertical overflow behavior. Default: [`Overflow::Hidden`].
    pub overflow_y: Overflow,
    /// The main axis children are laid out along. Default: [`Direction::Column`].
    pub direction: Direction,
    /// Wraps children onto new lines when they overflow the main axis. Default: `false`.
    pub wrap: bool,
    /// Space between adjacent children, and between wrapped lines. Default: 0.
    pub gap: f32,
    /// Space between the border and the content, on all four sides. Default: 0.
    pub padding: f32,
    /// Space outside the frame, on all four sides. Default: 0.
    pub margin: f32,
    /// Border thickness, on all four sides. Takes up layout space. Default: 0.
    pub border_width: f32,
    /// Border color. Default: black.
    pub border_color: Color,
    /// Preferred width. Default: [`Length::Auto`].
    pub width: Length,
    /// Preferred height. Default: [`Length::Auto`].
    pub height: Length,
    /// Minimum width. Default: 0.
    pub min_width: Length,
    /// Minimum height. Default: 0.
    pub min_height: Length,
    /// Maximum width. Default: [`Length::Auto`], meaning no limit.
    pub max_width: Length,
    /// Maximum height. Default: [`Length::Auto`], meaning no limit.
    pub max_height: Length,
    /// How much of the parent's leftover main-axis space this frame takes,
    /// relative to its siblings. Default: 0.
    pub grow: f32,
    /// How much this frame shrinks, relative to its siblings, when the parent
    /// is too small along the main axis. 0 means never. Default: 1.
    pub shrink: f32,
    /// How children are placed across the main axis. Default: [`Align::Stretch`].
    pub align: Align,
    /// How children are distributed along the main axis. Default: [`Justify::Start`].
    pub justify: Justify,
    /// Fill color. Default: none.
    pub background: Option<Color>,
    /// Fill color while hovered, replacing `background`. Default: none.
    pub hover_background: Option<Color>,
    /// Fill color while pressed, taking priority over the hover and focus
    /// backgrounds. Default: none.
    pub active_background: Option<Color>,
    /// Fill color while focused, unless hovered or pressed. Default: none.
    pub focus_background: Option<Color>,
    /// Radius of the background and border corners. Default: 0.
    pub corner_radius: u8,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            overflow_x: Overflow::Hidden,
            overflow_y: Overflow::Hidden,
            direction: Direction::Column,
            wrap: false,
            gap: 0.0,
            padding: 0.0,
            margin: 0.0,
            border_width: 0.0,
            border_color: Color::rgb(0, 0, 0),
            width: Length::Auto,
            height: Length::Auto,
            min_width: Length::Px(0.0),
            min_height: Length::Px(0.0),
            max_width: Length::Auto,
            max_height: Length::Auto,
            grow: 0.0,
            shrink: 1.0,
            align: Align::Stretch,
            justify: Justify::Start,
            background: None,
            hover_background: None,
            active_background: None,
            focus_background: None,
            corner_radius: 0,
        }
    }
}

macro_rules! setters {
    ($($name:ident: $ty:ty),* $(,)?) => {$ (
        #[doc = concat!("Sets [`", stringify!($name), "`](Self::", stringify!($name), ").")]
        pub fn $name(mut self, value: $ty) -> Self { self.$name = value; self }
    )*};
}

impl Style {
    /// The default style with [`Direction::Row`].
    pub fn row() -> Self {
        Self {
            direction: Direction::Row,
            ..Self::default()
        }
    }
    /// The default style, which lays out children in a column.
    pub fn column() -> Self {
        Self::default()
    }
    setters! { direction: Direction, wrap: bool, gap: f32, padding: f32, margin: f32,
    grow: f32, shrink: f32, align: Align, justify: Justify, corner_radius: u8,
    overflow_x: Overflow, overflow_y: Overflow }
    /// Sets [`background`](Self::background).
    pub fn background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }
    /// Sets [`border_width`](Self::border_width) and [`border_color`](Self::border_color).
    pub fn border(mut self, width: f32, color: Color) -> Self {
        self.border_width = width;
        self.border_color = color;
        self
    }
    /// Sets [`hover_background`](Self::hover_background).
    pub fn hover_background(mut self, color: Color) -> Self {
        self.hover_background = Some(color);
        self
    }
    /// Sets [`active_background`](Self::active_background).
    pub fn active_background(mut self, color: Color) -> Self {
        self.active_background = Some(color);
        self
    }
    /// Sets [`focus_background`](Self::focus_background).
    pub fn focus_background(mut self, color: Color) -> Self {
        self.focus_background = Some(color);
        self
    }
    /// Sets [`width`](Self::width).
    pub fn width(mut self, value: impl Into<Length>) -> Self {
        self.width = value.into();
        self
    }
    /// Sets [`height`](Self::height).
    pub fn height(mut self, value: impl Into<Length>) -> Self {
        self.height = value.into();
        self
    }
    /// Sets [`min_width`](Self::min_width).
    pub fn min_width(mut self, value: impl Into<Length>) -> Self {
        self.min_width = value.into();
        self
    }
    /// Sets [`min_height`](Self::min_height).
    pub fn min_height(mut self, value: impl Into<Length>) -> Self {
        self.min_height = value.into();
        self
    }
    /// Sets [`max_width`](Self::max_width).
    pub fn max_width(mut self, value: impl Into<Length>) -> Self {
        self.max_width = value.into();
        self
    }
    /// Sets [`max_height`](Self::max_height).
    pub fn max_height(mut self, value: impl Into<Length>) -> Self {
        self.max_height = value.into();
        self
    }

    pub(crate) fn taffy(&self) -> taffy::Style {
        use taffy::{
            AlignItems, Dimension, FlexDirection, FlexWrap, JustifyContent, LengthPercentage,
            Point, Rect, Size,
        };
        let dimension = |v| match v {
            Length::Auto => Dimension::auto(),
            Length::Px(v) => Dimension::length(v),
            Length::Percent(v) => Dimension::percent(v),
        };
        taffy::Style {
            display: taffy::Display::Flex,
            flex_direction: match self.direction {
                Direction::Row => FlexDirection::Row,
                Direction::Column => FlexDirection::Column,
            },
            flex_wrap: if self.wrap {
                FlexWrap::Wrap
            } else {
                FlexWrap::NoWrap
            },
            size: Size {
                width: dimension(self.width),
                height: dimension(self.height),
            },
            min_size: Size {
                width: dimension(self.min_width),
                height: dimension(self.min_height),
            },
            max_size: Size {
                width: dimension(self.max_width),
                height: dimension(self.max_height),
            },
            padding: Rect {
                left: LengthPercentage::length(self.padding),
                right: LengthPercentage::length(self.padding),
                top: LengthPercentage::length(self.padding),
                bottom: LengthPercentage::length(self.padding),
            },
            margin: Rect {
                left: taffy::LengthPercentageAuto::length(self.margin),
                right: taffy::LengthPercentageAuto::length(self.margin),
                top: taffy::LengthPercentageAuto::length(self.margin),
                bottom: taffy::LengthPercentageAuto::length(self.margin),
            },
            border: Rect {
                left: LengthPercentage::length(self.border_width),
                right: LengthPercentage::length(self.border_width),
                top: LengthPercentage::length(self.border_width),
                bottom: LengthPercentage::length(self.border_width),
            },
            gap: Size {
                width: LengthPercentage::length(self.gap),
                height: LengthPercentage::length(self.gap),
            },
            flex_grow: self.grow,
            flex_shrink: self.shrink,
            align_items: Some(match self.align {
                Align::Start => AlignItems::Start,
                Align::Center => AlignItems::Center,
                Align::End => AlignItems::End,
                Align::Stretch => AlignItems::Stretch,
            }),
            justify_content: Some(match self.justify {
                Justify::Start => JustifyContent::Start,
                Justify::Center => JustifyContent::Center,
                Justify::End => JustifyContent::End,
                Justify::SpaceBetween => JustifyContent::SpaceBetween,
                Justify::SpaceAround => JustifyContent::SpaceAround,
                Justify::SpaceEvenly => JustifyContent::SpaceEvenly,
            }),
            overflow: Point {
                x: taffy::Overflow::Hidden,
                y: taffy::Overflow::Hidden,
            },
            ..Default::default()
        }
    }
}
