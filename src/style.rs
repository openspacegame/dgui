//! Styling shared by every frame. All lengths are in egui points (logical
//! pixels).

/// A size along one axis. A plain `f32` converts to [`Length::Px`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Length {
    /// Sized by content and flex layout.
    #[default]
    Auto,
    /// A preferred size in points; flex layout can still shrink it.
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
    #[default]
    Row,
    /// Top to bottom.
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
    /// Content may extend outside this box, subject to ancestor clipping.
    #[default]
    Visible,
    /// Clip content without making the box a scroll container.
    Clip,
    /// A clipped scroll container with no user scrolling or visible bars.
    Hidden,
    /// Scroll overflowing content, showing bars only when necessary.
    Auto,
    /// Scroll content with a persistent scrollbar on this axis.
    Scroll,
}

impl Overflow {
    /// Whether this value establishes a scroll container for CSS sizing.
    pub fn is_scroll_container(self) -> bool {
        matches!(self, Self::Hidden | Self::Auto | Self::Scroll)
    }

    /// Whether the user can scroll this axis.
    pub fn user_scroll(self) -> bool {
        matches!(self, Self::Auto | Self::Scroll)
    }
}

/// Which box a specified width or height describes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BoxSizing {
    /// Include padding and borders, like a CSS border-box reset.
    #[default]
    BorderBox,
    /// Padding and borders are added outside the specified content size.
    ContentBox,
}

/// Placement of wrapped flex lines on the cross axis (`align-content`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AlignContent {
    /// Stretch lines to use the available cross-axis space.
    #[default]
    Stretch,
    /// Pack lines at the start.
    Start,
    /// Pack lines in the center.
    Center,
    /// Pack lines at the end.
    End,
    /// Distribute space between lines.
    SpaceBetween,
    /// Distribute space around lines.
    SpaceAround,
    /// Distribute equal space between lines and edges.
    SpaceEvenly,
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
/// Layout follows CSS flexbox with a `box-sizing: border-box` reset. Minimum
/// sizes are automatic and overflow is visible. Set `min_width(0.0)` to allow
/// a flex item to shrink below its intrinsic width, and choose clipping,
/// scrolling, or text wrapping explicitly. `width` is a preferred size, not
/// a prohibition on shrinking; use `shrink(0.0)` when that is the intent.
///
/// Every field has a builder method of the same name, also available on
/// `Frame`. Lengths accept an `f32` in points or a [`Length`].
///
/// ```
/// use dgui::{Align, Style};
///
/// let toolbar = Style::row().gap(8.0).padding(4.0).align(Align::Center);
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    /// Horizontal overflow behavior. Default: [`Overflow::Visible`].
    pub overflow_x: Overflow,
    /// Vertical overflow behavior. Default: [`Overflow::Visible`].
    pub overflow_y: Overflow,
    /// The main axis children are laid out along. Default: [`Direction::Row`].
    pub direction: Direction,
    /// Wraps children onto new lines when they overflow the main axis. Default: `false`.
    pub wrap: bool,
    /// Vertical spacing between children or wrapped rows. Default: zero.
    pub row_gap: f32,
    /// Horizontal spacing between children or wrapped columns. Default: zero.
    pub column_gap: f32,
    /// Padding in CSS order: top, right, bottom, left. Default: zero.
    pub padding: [f32; 4],
    /// Margins in CSS order: top, right, bottom, left. Auto absorbs free space.
    pub margin: [Length; 4],
    /// Interpretation of width and height. Default: border-box.
    pub box_sizing: BoxSizing,
    /// Border thickness, on all four sides. Takes up layout space. Default: 0.
    pub border_width: f32,
    /// Border color. Default: black.
    pub border_color: Color,
    /// Preferred width. Default: [`Length::Auto`].
    pub width: Length,
    /// Preferred height. Default: [`Length::Auto`].
    pub height: Length,
    /// Minimum width. Default: automatic content minimum.
    pub min_width: Length,
    /// Minimum height. Default: automatic content minimum.
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
    /// Initial main-axis size, before distributing free space. Default: auto.
    pub basis: Length,
    /// Override this item's cross-axis alignment. None uses its parent's align.
    pub align_self: Option<Align>,
    /// Placement of wrapped lines on the cross axis. Default: stretch.
    pub align_content: AlignContent,
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
            overflow_x: Overflow::Visible,
            overflow_y: Overflow::Visible,
            direction: Direction::Row,
            wrap: false,
            row_gap: 0.0,
            column_gap: 0.0,
            padding: [0.0; 4],
            margin: [Length::Px(0.0); 4],
            box_sizing: BoxSizing::BorderBox,
            border_width: 0.0,
            border_color: Color::rgb(0, 0, 0),
            width: Length::Auto,
            height: Length::Auto,
            min_width: Length::Auto,
            min_height: Length::Auto,
            max_width: Length::Auto,
            max_height: Length::Auto,
            grow: 0.0,
            shrink: 1.0,
            basis: Length::Auto,
            align_self: None,
            align_content: AlignContent::Stretch,
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
    /// A vertical flex container.
    pub fn column() -> Self {
        Self::default().direction(Direction::Column)
    }
    /// Set equal padding on every side.
    pub fn padding(self, value: f32) -> Self {
        self.padding_xy(value, value)
    }

    /// Set horizontal and vertical padding independently.
    pub fn padding_xy(mut self, horizontal: f32, vertical: f32) -> Self {
        self.padding = [vertical, horizontal, vertical, horizontal];
        self
    }

    /// Set equal gaps on both axes.
    pub fn gap(mut self, value: f32) -> Self {
        self.row_gap = value;
        self.column_gap = value;
        self
    }

    /// Set all margins, including auto margins.
    pub fn margin(mut self, value: impl Into<Length>) -> Self {
        self.margin = [value.into(); 4];
        self
    }

    /// Set padding in CSS order: top, right, bottom, left.
    pub fn padding_edges(mut self, value: [f32; 4]) -> Self {
        self.padding = value;
        self
    }

    /// Set margins in CSS order: top, right, bottom, left.
    pub fn margin_edges(mut self, value: [Length; 4]) -> Self {
        self.margin = value;
        self
    }

    /// Set both overflow axes.
    pub fn overflow(mut self, value: Overflow) -> Self {
        self.overflow_x = value;
        self.overflow_y = value;
        self
    }

    /// CSS normalizes visible/clip when paired with a scrollable overflow axis.
    pub fn effective_overflow(&self) -> [Overflow; 2] {
        let values = [self.overflow_x, self.overflow_y];
        if values.iter().any(|value| value.is_scroll_container()) {
            values.map(|value| match value {
                Overflow::Visible => Overflow::Auto,
                Overflow::Clip => Overflow::Hidden,
                other => other,
            })
        } else {
            values
        }
    }

    setters! { direction: Direction, wrap: bool, row_gap: f32, column_gap: f32,
    box_sizing: BoxSizing, align_self: Option<Align>, align_content: AlignContent,
    grow: f32, shrink: f32, align: Align, justify: Justify, corner_radius: u8,
    overflow_x: Overflow, overflow_y: Overflow }
    /// Set top padding.
    pub fn padding_top(mut self, value: f32) -> Self {
        self.padding[0] = value;
        self
    }

    /// Set the top margin, including auto.
    pub fn margin_top(mut self, value: impl Into<Length>) -> Self {
        self.margin[0] = value.into();
        self
    }

    /// Set right padding.
    pub fn padding_right(mut self, value: f32) -> Self {
        self.padding[1] = value;
        self
    }

    /// Set the right margin, including auto.
    pub fn margin_right(mut self, value: impl Into<Length>) -> Self {
        self.margin[1] = value.into();
        self
    }

    /// Set bottom padding.
    pub fn padding_bottom(mut self, value: f32) -> Self {
        self.padding[2] = value;
        self
    }

    /// Set the bottom margin, including auto.
    pub fn margin_bottom(mut self, value: impl Into<Length>) -> Self {
        self.margin[2] = value.into();
        self
    }

    /// Set left padding.
    pub fn padding_left(mut self, value: f32) -> Self {
        self.padding[3] = value;
        self
    }

    /// Set the left margin, including auto.
    pub fn margin_left(mut self, value: impl Into<Length>) -> Self {
        self.margin[3] = value.into();
        self
    }

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
    /// Set the initial main-axis flex size.
    pub fn basis(mut self, value: impl Into<Length>) -> Self {
        self.basis = value.into();
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
        let margin = |value| match value {
            Length::Auto => taffy::LengthPercentageAuto::auto(),
            Length::Px(value) => taffy::LengthPercentageAuto::length(value),
            Length::Percent(value) => taffy::LengthPercentageAuto::percent(value),
        };
        let overflow = self.effective_overflow().map(|value| match value {
            Overflow::Visible => taffy::Overflow::Visible,
            Overflow::Clip => taffy::Overflow::Clip,
            Overflow::Hidden => taffy::Overflow::Hidden,
            Overflow::Auto | Overflow::Scroll => taffy::Overflow::Scroll,
        });
        let align = |value| match value {
            Align::Start => AlignItems::FlexStart,
            Align::Center => AlignItems::Center,
            Align::End => AlignItems::FlexEnd,
            Align::Stretch => AlignItems::Stretch,
        };
        taffy::Style {
            display: taffy::Display::Flex,
            box_sizing: match self.box_sizing {
                BoxSizing::BorderBox => taffy::BoxSizing::BorderBox,
                BoxSizing::ContentBox => taffy::BoxSizing::ContentBox,
            },
            flex_basis: dimension(self.basis),
            align_self: self.align_self.map(align),
            align_content: Some(match self.align_content {
                AlignContent::Stretch => taffy::AlignContent::Stretch,
                AlignContent::Start => taffy::AlignContent::FlexStart,
                AlignContent::Center => taffy::AlignContent::Center,
                AlignContent::End => taffy::AlignContent::FlexEnd,
                AlignContent::SpaceBetween => taffy::AlignContent::SpaceBetween,
                AlignContent::SpaceAround => taffy::AlignContent::SpaceAround,
                AlignContent::SpaceEvenly => taffy::AlignContent::SpaceEvenly,
            }),
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
                left: LengthPercentage::length(self.padding[3]),
                right: LengthPercentage::length(self.padding[1]),
                top: LengthPercentage::length(self.padding[0]),
                bottom: LengthPercentage::length(self.padding[2]),
            },
            margin: Rect {
                left: margin(self.margin[3]),
                right: margin(self.margin[1]),
                top: margin(self.margin[0]),
                bottom: margin(self.margin[2]),
            },
            border: Rect {
                left: LengthPercentage::length(self.border_width),
                right: LengthPercentage::length(self.border_width),
                top: LengthPercentage::length(self.border_width),
                bottom: LengthPercentage::length(self.border_width),
            },
            gap: Size {
                width: LengthPercentage::length(self.column_gap),
                height: LengthPercentage::length(self.row_gap),
            },
            flex_grow: self.grow,
            flex_shrink: self.shrink,
            align_items: Some(align(self.align)),
            justify_content: Some(match self.justify {
                Justify::Start => JustifyContent::Start,
                Justify::Center => JustifyContent::Center,
                Justify::End => JustifyContent::End,
                Justify::SpaceBetween => JustifyContent::SpaceBetween,
                Justify::SpaceAround => JustifyContent::SpaceAround,
                Justify::SpaceEvenly => JustifyContent::SpaceEvenly,
            }),
            overflow: Point {
                x: overflow[0],
                y: overflow[1],
            },
            ..Default::default()
        }
    }
}
