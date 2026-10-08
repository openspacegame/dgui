//! A GUI built from a fresh frame tree on every render cycle.
//!
//! See the `demo` example for keyed components and Copy state handles.
//! Configure egui with `max_passes = 1` and call [`Dgui::show`] once per runtime
//! per frame. Callbacks run after the complete tree has been drawn.

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
pub use state::State;
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
    ordinals: HashMap<ScopePath, u64>,
}

/// Builder for a transient frame tree. Container closures execute now;
/// widget callbacks execute after layout and drawing.
pub struct Ui<'a, 'frame> {
    build: &'a mut Build<'frame>,
    scopes: &'a mut HashMap<ScopePath, Scope>,
    next_mount: &'a mut u64,
    runtime_id: u64,
    path: ScopePath,
    parent: usize,
}
impl<'frame> Ui<'_, 'frame> {
    /// Obtain an owned task handle for the current mounted scope.
    pub fn tasks(&mut self) -> Tasks {
        self.scopes
            .get_mut(&self.path)
            .unwrap()
            .tasks
            .get_or_insert_with(|| tasks::TaskSet::new(self.build.context.clone()))
            .handle()
    }

    /// Shorthand for adding a container frame in the current state scope.
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
    /// Add a frame, constructing its children now, before layout or interaction.
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

    /// Enter a keyed component scope without adding a layout frame.
    /// Sibling keys must be unique. Missing scopes unmount at the end of `show`.
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
    /// Get state local to this keyed scope. Values live until the scope unmounts.
    pub fn state<T: Send + Sync + 'static>(
        &mut self,
        key: impl Hash,
        init: impl FnOnce() -> T,
    ) -> State<T> {
        self.scopes
            .get_mut(&self.path)
            .unwrap()
            .state(hash(key), init)
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

/// Persistent state storage and backend integration for one tree.
/// The frame tree and its callbacks are discarded after every call to `show`.
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
    pub fn new() -> Self {
        Self::default()
    }

    /// Build, lay out, draw, and dispatch callbacks.
    /// The root fills the host's available rectangle. Use a finite host rectangle.
    /// Configure the host context with `max_passes = 1`. Call once per runtime
    /// per frame; callbacks execute once after the tree has been drawn.
    pub fn show<'frame>(&mut self, host: &mut egui::Ui, build: impl FnOnce(&mut Ui<'_, 'frame>)) {
        self.show_styled(
            host,
            Style::column()
                .width(Length::Percent(1.0))
                .height(Length::Percent(1.0)),
            build,
        );
    }

    /// Build a root with declarative sizing. Auto height follows its content;
    /// percentage dimensions fill the corresponding available host dimension.
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
