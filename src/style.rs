//! Styling shared by every frame. All lengths are logical pixels.

/// Percentages are fractions: `Percent(1.0)` fills the parent.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Length {
    #[default]
    Auto,
    Px(f32),
    Percent(f32),
}

impl From<f32> for Length {
    fn from(value: f32) -> Self {
        Self::Px(value)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Direction {
    Row,
    #[default]
    Column,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    Start,
    Center,
    End,
    #[default]
    Stretch,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Justify {
    #[default]
    Start,
    Center,
    End,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

/// How a bounded frame presents content exceeding its viewport.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Overflow {
    #[default]
    Hidden,
    Scroll,
}

/// An sRGB color with unmultiplied alpha.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color(pub u8, pub u8, pub u8, pub u8);

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self(r, g, b, 255)
    }
    pub(crate) fn egui(self) -> egui::Color32 {
        egui::Color32::from_rgba_unmultiplied(self.0, self.1, self.2, self.3)
    }
}

/// Layout and decoration for both containers and leaves.
/// Size constraints include padding. Overflow is clipped to the frame.
#[derive(Clone, Debug)]
pub struct Style {
    pub overflow_x: Overflow,
    pub overflow_y: Overflow,
    pub direction: Direction,
    pub wrap: bool,
    pub gap: f32,
    pub padding: f32,
    pub margin: f32,
    pub border_width: f32,
    pub border_color: Color,
    pub width: Length,
    pub height: Length,
    pub min_width: Length,
    pub min_height: Length,
    pub max_width: Length,
    pub max_height: Length,
    pub grow: f32,
    pub shrink: f32,
    pub align: Align,
    pub justify: Justify,
    pub background: Option<Color>,
    pub hover_background: Option<Color>,
    pub active_background: Option<Color>,
    pub focus_background: Option<Color>,
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
        pub fn $name(mut self, value: $ty) -> Self { self.$name = value; self }
    )*};
}

impl Style {
    pub fn row() -> Self {
        Self {
            direction: Direction::Row,
            ..Self::default()
        }
    }
    pub fn column() -> Self {
        Self::default()
    }
    setters! { direction: Direction, wrap: bool, gap: f32, padding: f32, margin: f32,
    grow: f32, shrink: f32, align: Align, justify: Justify, corner_radius: u8,
    overflow_x: Overflow, overflow_y: Overflow }
    pub fn background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }
    pub fn border(mut self, width: f32, color: Color) -> Self {
        self.border_width = width;
        self.border_color = color;
        self
    }
    pub fn hover_background(mut self, color: Color) -> Self {
        self.hover_background = Some(color);
        self
    }
    pub fn active_background(mut self, color: Color) -> Self {
        self.active_background = Some(color);
        self
    }
    pub fn focus_background(mut self, color: Color) -> Self {
        self.focus_background = Some(color);
        self
    }
    pub fn width(mut self, value: impl Into<Length>) -> Self {
        self.width = value.into();
        self
    }
    pub fn height(mut self, value: impl Into<Length>) -> Self {
        self.height = value.into();
        self
    }
    pub fn min_width(mut self, value: impl Into<Length>) -> Self {
        self.min_width = value.into();
        self
    }
    pub fn min_height(mut self, value: impl Into<Length>) -> Self {
        self.min_height = value.into();
        self
    }
    pub fn max_width(mut self, value: impl Into<Length>) -> Self {
        self.max_width = value.into();
        self
    }
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
