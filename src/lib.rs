//! A GUI built from a fresh frame tree on every render cycle.
//!
//! Each frame, your code builds the whole UI as a tree of [`Frame`]s. dgui lays
//! the tree out with flexbox ([Taffy](https://docs.rs/taffy)), draws it with
//! [egui](https://docs.rs/egui), runs the callbacks it collected, and throws the
//! tree away. Keyed state persists, and unchanged layout geometry is reused after
//! validating the current tree and its intrinsic measurements.
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
//!   canvases](Frame::canvas) and [virtual lists](crate::VirtualList) are all
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
//! # Layout
//!
//! Frames use CSS flex defaults with a border-box reset: rows, automatic
//! minimum sizes, visible overflow, and zero spacing. See [`Style`] and [`Text`].
//! `Dgui::show` supplies a full-size column host; `show_styled` chooses the host.
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
mod text;
mod virtual_list;
pub use builtins::{button, text_input};
pub use canvas::{AvailableSpace, Canvas, MeasureInput};
use frame::Events;
pub use frame::Frame;
pub use state::{State, StateRead};
pub use style::{
    Align, AlignContent, BoxSizing, Color, Direction, Justify, Length, Overflow, Style,
};
pub use tasks::Tasks;
pub use text::{OverflowWrap, Text, TextAlign, TextOverflow, WhiteSpace};
pub use virtual_list::VirtualList;

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

#[derive(Default)]
enum Content<'a> {
    #[default]
    Empty,
    Canvas(CanvasContent<'a>),
    Text(Text),
}

struct FrameNode<'a> {
    id: egui::Id,
    children: Vec<usize>,
    style: Style,
    content: Content<'a>,
    events: Events<'a>,
    clickable: bool,
    focusable: bool,
    enabled: bool,
    sense: Option<egui::Sense>,
    accessibility_label: Option<String>,
}
impl FrameNode<'_> {
    fn layout_only(&self) -> bool {
        matches!(self.content, Content::Empty)
            && self.events.click.is_none()
            && self.events.hover.is_none()
            && self.events.focus.is_none()
            && self.events.blur.is_none()
            && self.events.response.is_none()
            && !self.clickable
            && !self.focusable
            && self.sense.is_none()
            && self.accessibility_label.is_none()
            && self.style.background.is_none()
            && self.style.hover_background.is_none()
            && self.style.active_background.is_none()
            && self.style.focus_background.is_none()
            && self.style.border_width == 0.0
    }
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
        let parent = self.push(Frame::new().style(style));
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
        let parent = self.push(frame);
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
            content: frame.content,
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
/// [`show`](Self::show). State, tasks, and validated layout geometry persist.
/// Dropping the runtime cancels all its tasks and drops all its state.
pub struct Dgui {
    id: u64,
    next_mount: u64,
    layout_cache: LayoutCache,
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
            layout_cache: LayoutCache::default(),
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
                content: Content::Empty,
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
        let mut builder = Ui {
            build: &mut tree,
            scopes: &mut self.scopes,
            next_mount: &mut self.next_mount,
            runtime_id: self.id,
            path: Vec::new(),
            parent: 0,
        };
        build(&mut builder);

        // Taffy remains local because its nodes are not Send. The cache owns
        // only geometry and the inputs that justify reusing it.
        let mut layout = TaffyTree::new();
        // Preserve fractional UI points so measured text does not wrap again
        // after layout. egui handles physical-pixel rasterization.
        layout.disable_rounding();
        let mut ids = Vec::new();
        let size = [root_rect.width(), root_rect.height()];
        let layouts = if let Some(layouts) = self.layout_cache.reuse(size, &tree.nodes, host.ctx())
        {
            layouts
        } else {
            ids = tree
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
            // Native scroll containers can reflow during drawing. Their layout
            // stays transient; independent runtimes inside canvas leaves can cache.
            let cacheable = tree.nodes.iter().all(|node| {
                !node
                    .style
                    .effective_overflow()
                    .iter()
                    .any(|value| value.is_scroll_container())
            });
            let mut measurements = Vec::new();
            compute_layout(
                &mut layout,
                ids[0],
                Size {
                    width: TaffyAvailableSpace::Definite(size[0]),
                    height: TaffyAvailableSpace::Definite(size[1]),
                },
                &tree.nodes,
                host.ctx(),
                cacheable.then_some(&mut measurements),
            );
            let layouts: Vec<_> = ids.iter().map(|&id| *layout.layout(id).unwrap()).collect();
            if cacheable {
                self.layout_cache = LayoutCache {
                    size,
                    nodes: tree
                        .nodes
                        .iter()
                        .map(|node| (node.style.clone(), node.children.clone()))
                        .collect(),
                    measurements,
                    layouts: layouts.clone(),
                };
            } else {
                self.layout_cache = LayoutCache::default();
            }
            layouts
        };
        let mut callbacks: Vec<Callback<'frame>> = Vec::new();
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

#[derive(Default)]
struct LayoutCache {
    size: [f32; 2],
    nodes: Vec<(Style, Vec<usize>)>,
    measurements: Vec<Measurement>,
    layouts: Vec<taffy::Layout>,
}

impl LayoutCache {
    fn reuse(
        &self,
        size: [f32; 2],
        nodes: &[FrameNode<'_>],
        context: &egui::Context,
    ) -> Option<Vec<taffy::Layout>> {
        if self.size != size
            || self.nodes.len() != nodes.len()
            || self
                .nodes
                .iter()
                .zip(nodes)
                .any(|((style, children), node)| *style != node.style || *children != node.children)
        {
            return None;
        }
        // Canvas closures may capture new data on every draw. Re-evaluate the
        // constraints that established this geometry rather than caching closures.
        for measurement in &self.measurements {
            if measure(
                &nodes[measurement.index],
                context,
                measurement.known,
                measurement.available,
            ) != measurement.result
            {
                return None;
            }
        }
        Some(self.layouts.clone())
    }
}

struct Measurement {
    index: usize,
    known: Size<Option<f32>>,
    available: Size<TaffyAvailableSpace>,
    result: Size<f32>,
}

// Define the measurement closure outside generic UI builders so Taffy's layout
// algorithms share one specialization across windows and scroll reflows.
fn compute_layout(
    layout: &mut TaffyTree<usize>,
    root: taffy::NodeId,
    available: Size<TaffyAvailableSpace>,
    nodes: &[FrameNode<'_>],
    context: &egui::Context,
    mut measurements: Option<&mut Vec<Measurement>>,
) {
    layout
        .compute_layout_with_measure(root, available, |known, available, _, node_index, _| {
            let index = node_index.copied().unwrap();
            let result = measure(&nodes[index], context, known, available);
            if let Some(measurements) = &mut measurements {
                measurements.push(Measurement {
                    index,
                    known,
                    available,
                    result,
                });
            }
            result
        })
        .unwrap();
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
    let natural = match &node.content {
        Content::Empty => [0.0; 2],
        Content::Canvas(canvas) => (canvas.measure)(context, input),
        Content::Text(text) => text.measure(context, input),
    };
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
    parent_clip: Rect,
    clip: Rect,
    callbacks: &mut Vec<Callback<'a>>,
) {
    // Layout traversal still applies bounds, clipping, and disabled state to
    // descendants. An undecorated structural frame needs no egui widget itself.
    if node.layout_only() {
        return;
    }
    let mut ui = host.new_child(
        egui::UiBuilder::new()
            .id_salt(node.id)
            .max_rect(content_rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    ui.set_clip_rect(parent_clip);
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
    ui.set_clip_rect(clip);
    if let Content::Text(text) = &node.content {
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Label, ui.is_enabled(), text.value())
        });
        text.paint(&mut ui, content_rect, node.style.effective_overflow());
    }
    let mut canvas_context = Canvas {
        ui: &mut ui,
        id: node.id,
        frame_rect: rect,
        content_rect,
        frame_clip: parent_clip.intersect(rect),
        callbacks,
        response: response.clone(),
        content_response: None,
    };
    if let Content::Canvas(canvas) = &mut node.content
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
