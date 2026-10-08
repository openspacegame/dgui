//! A GUI built from a fresh frame tree on every render cycle.
//!
//! Each frame, your code builds the whole UI as a tree of [`Frame`]s. dgui lays
//! the tree out with flexbox ([Taffy](https://docs.rs/taffy)), draws it with
//! [egui](https://docs.rs/egui), runs the callbacks it collected, and throws the
//! tree away. Nothing is diffed or retained except the state you ask for.
//!
//! # Example
//!
//! ```no_run
//! use dgui::{Align, Dgui, Direction, Frame, button};
//!
//! struct App {
//!     gui: Dgui,
//! }
//!
//! impl eframe::App for App {
//!     fn ui(&mut self, host: &mut egui::Ui, _frame: &mut eframe::Frame) {
//!         self.gui.show(host, |ui| {
//!             let count = ui.state("count", || 0);
//!             ui.add(
//!                 Frame::new()
//!                     .direction(Direction::Row)
//!                     .align(Align::Center)
//!                     .gap(12.0)
//!                     .children(move |ui| {
//!                         ui.add(button("−").on_click(move || count.update(|n| *n -= 1)));
//!                         ui.add(Frame::text(format!("Count: {}", count.get())));
//!                         ui.add(button("+").on_click(move || count.update(|n| *n += 1)));
//!                     }),
//!             );
//!         });
//!     }
//! }
//!
//! fn main() -> eframe::Result {
//!     eframe::run_native(
//!         "counter",
//!         eframe::NativeOptions::default(),
//!         Box::new(|cc| {
//!             // dgui requires single-pass egui.
//!             cc.egui_ctx
//!                 .options_mut(|options| options.max_passes = 1.try_into().unwrap());
//!             Ok(Box::new(App { gui: Dgui::new() }))
//!         }),
//!     )
//! }
//! ```
//!
//! # Concepts
//!
//! - **Everything is a [`Frame`].** Containers, [text](Frame::text),
//!   [buttons](button), [text inputs](text_input), [custom
//!   canvases](Frame::canvas) and [virtual lists](Frame::virtual_list) are all
//!   frames, and all share the same [`Style`] and event methods.
//! - **Building happens before drawing.** [`Ui::add`] runs a frame's
//!   [`children`](Frame::children) closure immediately, so the complete tree
//!   exists before layout. Measurement and painting come later.
//! - **Callbacks run after drawing.** [`Frame::on_click`] and friends fire once
//!   the whole tree has been laid out and painted, so the tree never changes
//!   while it is being built or drawn. State they write is seen the next time
//!   the tree is built.
//! - **State is keyed, not retained.** [`Ui::state`] returns a [`State`] handle
//!   owned by the enclosing [`Ui::scope`]. A scope that isn't built during a
//!   call to [`Dgui::show`] unmounts, dropping its state and [`Tasks`].
//!
//! # Requirements
//!
//! - The egui context must use `max_passes = 1`; [`Dgui::show`] panics
//!   otherwise.
//! - Call `show` at most once per [`Dgui`] per egui frame, with a finite host
//!   rectangle.
#![warn(missing_docs)]

mod builtins;
mod canvas;
mod frame;
mod scroll;
mod state;
mod style;
mod tasks;
mod virtual_list;
pub use builtins::{button, text_input};
pub use canvas::{AvailableSpace, Canvas, MeasureInput};
use frame::Events;
pub use frame::Frame;
pub use state::{State, StateRead};
pub use style::{Align, Color, Direction, Justify, Length, Overflow, Style};
pub use tasks::Tasks;

use ahash::{AHashMap as HashMap, AHashSet as HashSet};
use canvas::CanvasContent;
use egui::Rect;
use state::Scope;
use std::{
    hash::{Hash, Hasher},
    sync::atomic::{AtomicU64, Ordering},
};
use taffy::{AvailableSpace as TaffyAvailableSpace, Size, TaffyTree};

type Callback<'a> = Box<dyn FnOnce() + 'a>;
type ScopePath = Vec<u64>;

struct FrameNode<'a> {
    id: egui::Id,
    children: Vec<usize>,
    style: Style,
    canvas: Option<CanvasContent<'a>>,
    events: Events<'a>,
    clickable: bool,
    focusable: bool,
    enabled: bool,
    sense: Option<egui::Sense>,
    accessibility_label: Option<String>,
}
struct Build<'a> {
    context: egui::Context,
    nodes: Vec<FrameNode<'a>>,
    visited: HashSet<ScopePath>,
    state_keys: HashSet<(u64, u64)>,
    ordinals: HashMap<ScopePath, u64>,
}

/// Builder for the frame tree, passed to [`Dgui::show`] and to every
/// [`Frame::children`] closure.
///
/// A `Ui` adds children to one parent frame and declares state in one keyed
/// scope. Container closures run immediately; event callbacks run after the
/// whole tree has been laid out and drawn.
///
/// `'frame` is the lifetime of the tree itself: closures given to frames may
/// borrow anything that outlives the call to [`Dgui::show`].
pub struct Ui<'a, 'frame> {
    build: &'a mut Build<'frame>,
    scopes: &'a mut HashMap<ScopePath, Scope>,
    next_mount: &'a mut u64,
    runtime_id: u64,
    path: ScopePath,
    parent: usize,
}
impl<'frame> Ui<'_, 'frame> {
    /// Returns the [`Tasks`] handle of the current scope, for running async
    /// work that is cancelled when the scope unmounts.
    pub fn tasks(&mut self) -> Tasks {
        self.scopes
            .get_mut(&self.path)
            .unwrap()
            .tasks
            .get_or_insert_with(|| tasks::TaskSet::new(self.build.context.clone()))
            .handle()
    }

    /// Adds a container with the given style and builds its children now.
    ///
    /// Equivalent to `ui.add(Frame::new().style(style).children(children))`,
    /// except that `children` need not be `'frame`, so it can borrow locals.
    /// The children stay in the current state scope.
    pub fn frame(&mut self, style: Style, children: impl FnOnce(&mut Ui<'_, 'frame>)) {
        let parent = self.push_container(Frame::new().style(style));
        children(&mut Ui {
            build: self.build,
            scopes: self.scopes,
            next_mount: self.next_mount,
            runtime_id: self.runtime_id,
            path: self.path.clone(),
            parent,
        });
    }
    /// Appends `frame` as the next child of the current parent.
    ///
    /// If the frame has [`children`](Frame::children), that closure runs now,
    /// before this call returns and before any layout or interaction.
    pub fn add(&mut self, mut frame: Frame<'frame>) {
        let children = frame.children.take();
        let parent = if children.is_some() {
            self.push_container(frame)
        } else {
            self.push(frame)
        };
        if let Some(children) = children {
            children(&mut Ui {
                build: self.build,
                scopes: self.scopes,
                next_mount: self.next_mount,
                runtime_id: self.runtime_id,
                path: self.path.clone(),
                parent,
            });
        }
    }

    /// Builds `children` inside a keyed component scope. The scope adds no
    /// frame of its own; its children join the current parent.
    ///
    /// A scope owns the [`State`]s declared in it and its [`Tasks`]. It is
    /// identified by its key path from the root, so it keeps that state across
    /// frames even if it moves among its siblings. A scope that isn't built
    /// during a call to [`Dgui::show`] unmounts at the end of that call.
    ///
    /// Scopes also anchor widget identity: frames are identified by their
    /// order within their scope, so wrapping list items in keyed scopes keeps
    /// focus and scroll positions attached to the right item.
    ///
    /// # Panics
    ///
    /// If two sibling scopes in one draw use the same key.
    ///
    /// # Example
    ///
    /// ```
    /// # fn build(ui: &mut dgui::Ui<'_, '_>, users: &[(u64, String)]) {
    /// for (id, name) in users {
    ///     ui.scope(id, |ui| {
    ///         let draft = ui.state("draft", || name.clone());
    ///         ui.add(dgui::text_input(draft));
    ///     });
    /// }
    /// # }
    /// ```
    pub fn scope(&mut self, key: impl Hash, children: impl FnOnce(&mut Ui<'_, 'frame>)) {
        let mut path = self.path.clone();
        path.push(hash(key));
        assert!(
            self.build.visited.insert(path.clone()),
            "dgui: duplicate sibling scope key"
        );
        self.scopes.entry(path.clone()).or_insert_with(|| {
            *self.next_mount += 1;
            Scope::new(*self.next_mount)
        });
        let mut child = Ui {
            build: self.build,
            scopes: self.scopes,
            next_mount: self.next_mount,
            runtime_id: self.runtime_id,
            path,
            parent: self.parent,
        };
        children(&mut child);
    }
    /// Declares state local to the current scope and returns its handle.
    ///
    /// The first time `key` is declared in a mounted scope, `init` creates the
    /// value; later frames return the same value and don't call `init`. The
    /// value is dropped when the scope unmounts. State declared outside any
    /// [`scope`](Self::scope) lives as long as the [`Dgui`].
    ///
    /// Declare each key once per scope per draw. To use the state elsewhere,
    /// pass the `Copy` handle along instead of declaring it again.
    ///
    /// # Panics
    ///
    /// If `key` was already declared in this scope during this draw, or was
    /// declared earlier with a different type `T`.
    #[track_caller]
    pub fn state<T: Send + Sync + 'static>(
        &mut self,
        key: impl Hash,
        init: impl FnOnce() -> T,
    ) -> State<T> {
        let key = hash(key);
        assert!(
            self.build
                .state_keys
                .insert((self.scopes[&self.path].mount, key)),
            "dgui: duplicate state key in the same scope during one draw; pass the State handle instead"
        );
        self.scopes.get_mut(&self.path).unwrap().state(key, init)
    }
    fn push(&mut self, frame: Frame<'frame>) -> usize {
        let ordinal = self.build.ordinals.entry(self.path.clone()).or_default();
        let id = egui::Id::new((self.runtime_id, self.scopes[&self.path].mount, *ordinal));
        *ordinal += 1;
        let index = self.build.nodes.len();
        self.build.nodes.push(FrameNode {
            id,
            children: Vec::new(),
            style: frame.style,
            canvas: frame.canvas,
            events: frame.events,
            clickable: frame.clickable,
            focusable: frame.focusable,
            enabled: self.build.nodes[self.parent].enabled && !frame.disabled,
            sense: frame.sense,
            accessibility_label: frame.accessibility_label,
        });
        self.build.nodes[self.parent].children.push(index);
        index
    }

    fn push_container(&mut self, mut frame: Frame<'frame>) -> usize {
        let Some(content) = scroll::content_style(&mut frame.style) else {
            return self.push(frame);
        };
        let outer = self.push(frame);
        let previous = self.parent;
        self.parent = outer;
        let inner = self.push(Frame::new().style(content));
        self.parent = previous;
        inner
    }
}
fn hash(key: impl Hash) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut hasher);
    hasher.finish()
}

/// A dgui runtime: holds the persistent state of one tree and draws it into
/// an egui [`Ui`](egui::Ui).
///
/// Keep one `Dgui` per independent tree, typically as a field of your app.
/// The frame tree and its callbacks are discarded after every call to
/// [`show`](Self::show); only [`State`]s and [`Tasks`] persist. Dropping the
/// runtime cancels all its tasks and drops all its state.
pub struct Dgui {
    id: u64,
    next_mount: u64,
    scopes: HashMap<ScopePath, Scope>,
    #[cfg(test)]
    last_layout: Vec<(egui::Id, Rect)>,
}
impl Drop for Dgui {
    fn drop(&mut self) {
        // Child tasks can capture ancestor states. Cancel every task before
        // releasing any scope, regardless of the map's destruction order.
        for scope in self.scopes.values_mut() {
            scope.tasks = None;
        }
    }
}
impl Default for Dgui {
    fn default() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            next_mount: 0,
            scopes: HashMap::from([(Vec::new(), Scope::new(0))]),
            #[cfg(test)]
            last_layout: Vec::new(),
        }
    }
}
impl Dgui {
    /// Creates a runtime with no state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds, lays out and draws one frame of the tree, then runs callbacks.
    ///
    /// In order, this:
    /// 1. calls `build` to construct the complete tree,
    /// 2. lays it out with a root column that fills the host's available
    ///    rectangle,
    /// 3. draws it into `host`,
    /// 4. runs the event callbacks collected while drawing,
    /// 5. unmounts every scope that `build` didn't visit, and
    /// 6. polls the remaining scopes' [`Tasks`].
    ///
    /// Call this at most once per runtime per egui frame. Use
    /// [`show_styled`](Self::show_styled) to style or size the root.
    ///
    /// # Panics
    ///
    /// If the egui context's `max_passes` isn't 1, or the host's available
    /// rectangle is infinite (e.g. directly inside a scroll area).
    pub fn show<'frame>(&mut self, host: &mut egui::Ui, build: impl FnOnce(&mut Ui<'_, 'frame>)) {
        self.show_styled(
            host,
            Style::column()
                .width(Length::Percent(1.0))
                .height(Length::Percent(1.0)),
            build,
        );
    }

    /// Like [`show`](Self::show), but with `style` on the root frame.
    ///
    /// Percentage sizes are fractions of the host's available rectangle, and
    /// [`Length::Auto`] sizes the root to its content. The host's cursor
    /// advances past the root, so an auto-height root can be followed by
    /// other egui widgets.
    pub fn show_styled<'frame>(
        &mut self,
        host: &mut egui::Ui,
        style: Style,
        build: impl FnOnce(&mut Ui<'_, 'frame>),
    ) {
        assert_eq!(
            host.ctx().options(|options| options.max_passes.get()),
            1,
            "dgui requires egui max_passes = 1"
        );
        let root_rect = host.available_rect_before_wrap();
        assert!(
            root_rect.is_finite(),
            "dgui requires a finite host rectangle"
        );
        let mut tree = Build {
            context: host.ctx().clone(),
            nodes: vec![FrameNode {
                id: egui::Id::new((self.id, "root")),
                children: Vec::new(),
                style,
                canvas: None,
                events: Events::default(),
                clickable: false,
                focusable: false,
                enabled: host.is_enabled(),
                sense: None,
                accessibility_label: None,
            }],
            visited: HashSet::from([Vec::new()]),
            state_keys: HashSet::new(),
            ordinals: HashMap::new(),
        };
        let content_style = scroll::content_style(&mut tree.nodes[0].style);
        let mut builder = Ui {
            build: &mut tree,
            scopes: &mut self.scopes,
            next_mount: &mut self.next_mount,
            runtime_id: self.id,
            path: Vec::new(),
            parent: 0,
        };
        if let Some(style) = content_style {
            builder.parent = builder.push(Frame::new().style(style));
        }
        build(&mut builder);

        // Taffy owns transient layout nodes and does not implement Send. Keep
        // it local while persistent component state remains thread-safe.
        let mut layout = TaffyTree::new();
        // Layout uses egui points, which can cover fractional physical pixels.
        // Rounding here can narrow an intrinsically measured text leaf enough
        // to wrap another line after its height has already been determined.
        // Preserve point geometry; egui handles physical-pixel rasterization.
        layout.disable_rounding();
        let ids: Vec<_> = tree
            .nodes
            .iter()
            .enumerate()
            .map(|(index, node)| {
                layout
                    .new_leaf_with_context(node.style.taffy(), index)
                    .unwrap()
            })
            .collect();
        for (index, node) in tree.nodes.iter().enumerate() {
            let children: Vec<_> = node.children.iter().map(|&child| ids[child]).collect();
            layout.set_children(ids[index], &children).unwrap();
        }
        layout
            .compute_layout_with_measure(
                ids[0],
                Size {
                    width: TaffyAvailableSpace::Definite(root_rect.width()),
                    height: TaffyAvailableSpace::Definite(root_rect.height()),
                },
                |known, available, _, context, _| {
                    let node = &tree.nodes[context.copied().unwrap()];
                    measure(node, host.ctx(), known, available)
                },
            )
            .unwrap();

        let mut callbacks: Vec<Callback<'frame>> = Vec::new();
        let layouts: Vec<_> = ids.iter().map(|&id| *layout.layout(id).unwrap()).collect();
        let mut drawing = scroll::Drawing {
            nodes: &mut tree.nodes,
            layouts,
            layout: &mut layout,
            ids: &ids,
            callbacks: &mut callbacks,
            #[cfg(test)]
            rectangles: Vec::new(),
        };
        let painted_root = drawing.draw(host, 0, root_rect.min, host.clip_rect());
        #[cfg(test)]
        {
            self.last_layout = drawing.rectangles;
        }
        host.advance_cursor_after_rect(painted_root);
        for callback in callbacks {
            callback();
        }
        for (path, scope) in &mut self.scopes {
            if !tree.visited.contains(path) {
                scope.tasks = None;
            }
        }
        self.scopes.retain(|path, _| tree.visited.contains(path));
        for scope in self.scopes.values_mut() {
            if let Some(tasks) = &mut scope.tasks {
                tasks.poll();
            }
        }
    }
}

fn measure(
    node: &FrameNode<'_>,
    context: &egui::Context,
    known: Size<Option<f32>>,
    available: Size<TaffyAvailableSpace>,
) -> Size<f32> {
    let input = MeasureInput {
        known: [known.width, known.height],
        available: [available.width, available.height].map(|space| match space {
            TaffyAvailableSpace::Definite(value) => AvailableSpace::Definite(value),
            TaffyAvailableSpace::MinContent => AvailableSpace::MinContent,
            TaffyAvailableSpace::MaxContent => AvailableSpace::MaxContent,
        }),
    };
    let natural = node
        .canvas
        .as_ref()
        .map_or([0.0; 2], |canvas| (canvas.measure)(context, input));
    Size {
        width: known.width.unwrap_or(natural[0]),
        height: known.height.unwrap_or(natural[1]),
    }
}

fn render<'a>(
    host: &mut egui::Ui,
    node: &mut FrameNode<'a>,
    rect: Rect,
    content_rect: Rect,
    clip: Rect,
    callbacks: &mut Vec<Callback<'a>>,
) {
    let mut ui = host.new_child(
        egui::UiBuilder::new()
            .id_salt(node.id)
            .max_rect(content_rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    ui.set_clip_rect(clip);
    if !node.enabled {
        ui.disable();
    }
    let mut sense = node.sense.unwrap_or_else(egui::Sense::hover);
    if node.events.click.is_some() || node.clickable || node.focusable {
        sense |= egui::Sense::CLICK;
    }
    if node.focusable {
        sense |= egui::Sense::FOCUSABLE;
    }
    let response = ui.interact(rect, node.id.with("frame"), sense);
    if let Some(label) = &node.accessibility_label {
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Other, node.enabled, label)
        });
    }
    let native_previous = host.ctx().read_response(node.id);
    let hovered = response.hovered() || native_previous.as_ref().is_some_and(|r| r.hovered());
    let focused = response.has_focus() || native_previous.as_ref().is_some_and(|r| r.has_focus());
    let pressed = response.is_pointer_button_down_on()
        || native_previous
            .as_ref()
            .is_some_and(|r| r.is_pointer_button_down_on());
    let background = if pressed {
        node.style.active_background
    } else if hovered {
        node.style.hover_background
    } else if focused {
        node.style.focus_background
    } else {
        None
    };
    if let Some(color) = background.or(node.style.background) {
        ui.painter()
            .rect_filled(rect, node.style.corner_radius, color.egui());
    }
    if node.style.border_width > 0.0 {
        ui.painter().rect_stroke(
            rect,
            node.style.corner_radius,
            egui::Stroke::new(node.style.border_width, node.style.border_color.egui()),
            egui::StrokeKind::Inside,
        );
    }
    ui.set_clip_rect(clip.intersect(content_rect));
    let mut canvas_context = Canvas {
        ui: &mut ui,
        id: node.id,
        frame_rect: rect,
        content_rect,
        frame_clip: clip,
        callbacks,
        response: response.clone(),
        content_response: None,
    };
    if let Some(canvas) = &mut node.canvas
        && let Some(paint) = canvas.paint.take()
    {
        paint(&mut canvas_context);
    }
    let content_response = canvas_context.content_response.take();
    let clicked = response.clicked() || content_response.as_ref().is_some_and(|r| r.clicked());
    let hovered = response.hovered() || content_response.as_ref().is_some_and(|r| r.hovered());
    let gained_focus =
        response.gained_focus() || content_response.as_ref().is_some_and(|r| r.gained_focus());
    let lost_focus =
        response.lost_focus() || content_response.as_ref().is_some_and(|r| r.lost_focus());
    let combined = content_response.map_or_else(
        || response.clone(),
        |content| response.clone().union(content),
    );
    for (triggered, callback) in [
        (clicked, &mut node.events.click),
        (hovered, &mut node.events.hover),
        (gained_focus, &mut node.events.focus),
        (lost_focus, &mut node.events.blur),
    ] {
        if triggered
            && node.enabled
            && let Some(callback) = callback.take()
        {
            canvas_context.callbacks.push(callback);
        }
    }
    if let Some(callback) = node.events.response.take() {
        canvas_context
            .callbacks
            .push(Box::new(move || callback(combined)));
    }
}

#[cfg(test)]
mod tests;
