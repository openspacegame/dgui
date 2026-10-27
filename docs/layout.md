# Layout for CSS users

Every frame is a flex container or a measured leaf. `Style` and `Frame` expose
one set of constraints; the short property names are intentional.

| dgui | CSS equivalent | Default |
|---|---|---|
| `direction` | `flex-direction` | row |
| `wrap` | `flex-wrap` | false / nowrap |
| `grow`, `shrink`, `basis` | `flex-grow`, `flex-shrink`, `flex-basis` | 0, 1, auto |
| `align`, `align_self` | `align-items`, `align-self` | stretch, inherit parent's align |
| `justify`, `align_content` | `justify-content`, `align-content` | start, stretch |
| `row_gap`, `column_gap` | `row-gap`, `column-gap` | zero |
| `gap` | both gaps | zero |
| `padding`, `margin` | four edges, top/right/bottom/left | zero |
| `min_width`, `min_height` | automatic flex minimum | auto |
| `overflow_x`, `overflow_y` | overflow on each axis | visible |
| `box_sizing` | `box-sizing` | border-box reset |

`Dgui::show` mounts a full-size **column** as a convenience for vertical screens.
Use `show_styled` to choose its layout. Ordinary `Frame::new()` and
`Style::default()` are rows. `Style::column()` explicitly stacks children.

Sizes are logical egui points, independent of native DPI and application zoom.
`Length::Percent(1.0)` is 100%, resolved against the parent. Width and height are
preferred sizes: flex shrinking still applies. To forbid shrinking, use
`shrink(0.0)`. `basis(0.0).grow(1.0)` distributes remaining main-axis space equally;
add `min_width(0.0)` or `min_height(0.0)` when content may be smaller than its
intrinsic minimum. A scroll viewport needs a bounded axis.

Border-box sizes include padding and borders. `BoxSizing::ContentBox` adds them
outside the declared dimensions. `padding_edges` and `margin_edges` take CSS
order. Individual edge setters override that edge; `margin_left(Length::Auto)`
absorbs free space to push an action right. Components own internal spacing;
callers own gaps around them.

## Overflow

- `Visible` paints and receives input outside the frame, within ancestor clips.
- `Clip` clips at the padding box without changing automatic minimum sizing.
- `Hidden` clips and uses the scroll-container automatic minimum (zero), with
  no user scrolling. dgui currently has no programmatic scroll-offset API.
- `Auto` allows scrolling and shows a bar when content overflows.
- `Scroll` allows scrolling and always shows a bar, including empty content.

As in CSS, if either axis is Hidden, Auto, or Scroll, Visible on the other axis
computes to Auto and Clip computes to Hidden. Use `overflow(Clip)` for clipping
without scrolling. A vertical viewport with no horizontal scrolling can use
`overflow_y(Auto).overflow_x(Hidden)`.

Scrolling does not insert a hidden flex wrapper or disable flex shrinking.
Declare `shrink(0.0)` on rows that must retain a preferred height. Wrapped text
retains its automatic minimum height. Nested scroll areas consume wheel input
inside-out and propagate it at their limits. Layout uses the actual native
scrollbar gutter, including in virtual lists.

## Text

`Text::new(value).frame()` is a first-class measured leaf. It uses the current
Body style, collapses whitespace, wraps at Unicode line breaks, and measures its
minimum width from unbreakable words. Long words overflow unless the caller
chooses `overflow_wrap(OverflowWrap::Anywhere)`, which permits grapheme breaks.
Measurement and painting share the same line-breaking implementation.

`WhiteSpace` supports Normal, NoWrap, Pre, PreWrap, and PreLine. Explicit newlines
need PreLine or PreWrap. Nonbreaking spaces stay nonbreaking. Font family,
semantic font style, weight supplied through a font family, color, and horizontal
alignment are explicit on each text leaf. Typography does not inherit through
frames. `TextAlign` supports Start, Center, and End in horizontal LTR layout.

```rust
use dgui::{Frame, Length, Overflow, Text, TextOverflow, WhiteSpace};

// A shrinkable table cell with an ellipsis. Intrinsic width still describes
// the full text; min_width explicitly permits the flex item to shrink.
let cell = Text::new("A long destination name")
    .white_space(WhiteSpace::NoWrap)
    .text_overflow(TextOverflow::Ellipsis)
    .frame()
    .min_width(0.0)
    .overflow(Overflow::Clip);

// Equal columns with values that can wrap independently.
let column = Frame::new().basis(0.0).grow(1.0).min_width(0.0);
let viewport = Frame::new().height(240.0)
    .overflow_y(Overflow::Auto).overflow_x(Overflow::Hidden);
let right_action = Frame::new().margin_left(Length::Auto);
```

Custom canvases measure only their own content; they receive the final content
rectangle and the effective ancestor clip. Build ordinary composite content
from child frames. Taffy is internal; application code should not run layout,
measure composite heights, or move egui cursors to compensate for layout.

This is a flexbox model with Rust builders. It does not implement document flow,
Grid, CSS parsing/cascade, inherited typography, or font-baseline alignment.
