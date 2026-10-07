# dgui

A small Rust GUI prototype for applications that rebuild their UI every render cycle, such as games. **Every element is a `Frame`**. Containers, text, buttons, editors, and custom graphics share the same styling and event methods. Components are ordinary Rust functions returning frames.

```sh
cargo run --example demo
```

The native eframe demo includes a counter, editable crew cards, reordering and unmounting controls, wrapping text, and a clickable animated canvas. Resizing wraps the cards. Their names and scores follow their keys; removing and recreating a card resets its state.

## One element type

```rust
use dgui::{button, text_input, Color, Frame, State};

fn form<'a>(name: State<String>, save: impl FnOnce() + 'a) -> Frame<'a> {
    Frame::new()
        .padding(16.0)
        .margin(8.0)
        .gap(12.0)
        .background(Color::rgb(25, 35, 50))
        .border(1.0, Color::rgb(70, 90, 120))
        .children(move |ui| {
            ui.add(Frame::text("Pilot name"));
            ui.add(text_input(name).padding(10.0).margin(4.0));
            ui.add(button("Save").padding(12.0).on_click(save));
        })
}
```

`ui.add` accepts a `Frame`. `ui.frame(style, children)` is shorthand for creating and adding a container. Its child closure executes immediately. A `Frame::new().children(...)` closure executes when the frame is added, before layout.

Constructors all return the same type:

- `Frame::new()` creates an empty container.
- `Frame::text(text)` creates wrapping text content.
- `Frame::canvas(measure, draw)` creates measured custom content.
- `Frame::egui_canvas(size, draw)` creates a canvas with a fixed preferred size.
- `button(text)` creates a focusable frame containing centered text, with default colors and padding.
- `text_input(state)` creates a frame containing native single-line editing behavior.

There is no `Widget` trait or separate button/editor/canvas element type. These constructors can be used in the same `Vec<Frame<'_>>`, styled with a shared function, and composed with the same methods. `children(...)` replaces existing content, including a button's default label or a canvas; put text and canvas frames among the children to combine them.

## Shared styling and interaction

Every frame supports direction, flex wrapping, gap, padding, margin, width/height, min/max dimensions, grow/shrink, alignment, justification, backgrounds, borders, and corner radius directly. `Style` provides the same properties for reusable styles; `.style(style)` replaces the whole style, while individual setters change just one property.

All lengths are logical pixels. Dimension setters accept pixel numbers or `Length::Percent(1.0)` for 100%. Sizes include padding and border, and exclude margin. Padding, margin, and border width are currently uniform on all sides. Flex margins do not collapse. Margin lies outside painting and interaction. Frames clip rectangular overflow to themselves and their ancestors; corner radius rounds decorations but does not create a rounded clipping mask. There is no scrolling yet.

All frames also support:

- `.on_click(...)`: deferred pointer activation. Nested targets receive clicks without bubbling to ancestors. Clicking a noninteractive text child still activates its interactive parent.
- `.on_hover(...)`: a deferred callback each render cycle while hovered.
- `.on_focus(...)` and `.on_blur(...)`: focus transition callbacks.
- `.focusable(true)`: tab focus and Enter/Space activation for a frame. Buttons enable this by default. Plain clickable frames do not become keyboard targets automatically.
- `.clickable(true)`: reserve a pointer target even without a handler, as used for editor padding.
- `.hover_background(...)`, `.active_background(...)`, and `.focus_background(...)`: common state-dependent decorations.

Handlers run after the complete tree is drawn. An `on_click` setter replaces the previous handler. Callbacks may borrow data for the current render cycle; copied `State<T>` handles are convenient for sharing mutable application state.

Text inputs have specialized caret, selection, and keyboard behavior inside their common frame. Their generic click handlers also observe clicks on the editor. Clicking their padding focuses the editor. Typing a space does not trigger a frame click.

## Components and keyed state

Keep a `Dgui` instance in the host. Configure egui for one pass, then call `show` once per render cycle in a finite host region:

```rust,ignore
// Once during initialization:
ctx.options_mut(|options| options.max_passes = 1.try_into().unwrap());

// In eframe::App::ui:
host.ctx().request_repaint();
self.gui.show(host, |ui| {
    ui.scope("form", |ui| {
        let name = ui.state("name", String::new);
        ui.add(form(name, move || println!("Save {}", name.get())));
    });
});
```

Component functions can accept `&mut Ui<'_, 'a>` to obtain keyed state and return `Frame<'a>`. Store the result in a local before `ui.add` to avoid borrowing `ui` twice in one expression. See [`examples/demo.rs`](examples/demo.rs).

`ui.scope(key, children)` establishes component identity without adding a layout frame. Its identity is its parent scope plus its key. Sibling keys must be unique; use domain IDs for dynamic lists. Frames do not implicitly introduce state scopes.

`ui.state(key, initializer)` initializes once per scope. Repeated calls retrieve the same handle; changing the key's value type is an error. `State<T>` is `Copy` even when `T` is not:

- `get()` clones the value.
- `with(|value| ...)` reads without cloning.
- `set(value)` replaces it.
- `update(|value| ...)` mutates in place and returns the closure's result.

Handles use `generational-box` with runtime borrow checks on the UI thread. Unrelated states can be accessed within an update. Conflicting borrows of the same state and access after unmount panic.

A scope stays mounted while its closure appears in the build. State survives skipped state calls within a mounted scope. Missing scopes and their descendants are removed after callbacks, dropping values and invalidating handles. Reappearing scopes initialize fresh state. Root state lives until the `Dgui` instance is dropped.

Interaction identity uses mount generation and declaration order within the scope. Reordering keyed scopes preserves editor focus and selection. Adding or removing earlier unkeyed frames can shift identities; put independently movable or conditional components in keyed scopes. Remounting creates fresh identity.

## Custom canvas content

`Frame::canvas(measure, draw)` supplies independent measurement and drawing:

- `measure(&egui::Context, MeasureInput) -> [f32; 2]` receives known content dimensions and available space, including min-content and max-content requests. Return a preferred size excluding padding and border. The runtime enforces known dimensions. Measurement may execute repeatedly and must not paint, interact, or mutate application state. The context provides font measurement.
- `draw(&mut Canvas)` executes once after all layout. `canvas.ui` is constrained and clipped to the content rectangle. `id`, `frame_rect`, and `content_rect` expose native content identity and geometry.
- `canvas.response()` exposes the common frame interaction. Configure it through frame event methods. `canvas.frame_painter()` paints custom decorations, including padding, within ancestor clipping.
- `canvas.defer(callback)` queues an effect after drawing.
- `canvas.respond(response)` reports the response of a primary native control, using `canvas.id` for that control. Its interactions contribute to the enclosing frame's event handlers; padding clicks forward focus. The built-in editor uses this bridge.

`Frame::egui_canvas(size, draw)` is simpler: its closure receives just `&mut egui::Ui`. Raw egui controls added there keep their immediate interaction semantics. Use the measured canvas and `respond` when integrating native content with common frame handlers.

## Render lifecycle and limits

1. Build the complete transient frame tree and snapshot displayed values.
2. Measure content and compute rectangles with Taffy.
3. Apply frame decoration and interaction; draw content and children through egui.
4. Commit native edits and run frame callbacks in traversal order.
5. Unmount absent scopes and discard the tree and closures.

Native editors draw their temporary edit immediately and commit afterward; dependent content sees the change in the following tree. State methods themselves mutate immediately, so keep construction free of application mutations for a consistent snapshot.

The runtime retains keyed state and Taffy allocation capacity. Egui retains font and interaction caches. There is no tree diff, dependency graph, or subscription system. `show` requires one egui pass to prevent replaying effects. Native content hover styling may use the preceding pass's response; event handlers use its current response.

```sh
cargo test
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

This prototype targets native desktop, a fixed 16-pixel text style, and single-line editing. It has no scrolling, virtualization, controller navigation, multiline editor, persistence, or animations API. No performance claims have been benchmarked.
