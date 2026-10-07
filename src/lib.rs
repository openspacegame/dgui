//! A GUI built from a fresh frame tree on every render cycle.
//!
//! See the `demo` example for keyed components and Copy state handles.
//! Configure the host for one egui pass per render cycle. Call [`Dgui::show`]
//! once per runtime per cycle; callbacks execute after its tree is painted.

mod builtins;
mod canvas;
mod frame;
mod state;
mod style;
pub use builtins::{button, text_input};
pub use canvas::{AvailableSpace, Canvas, MeasureInput};
use frame::Events;
pub use frame::Frame;
pub use state::State;
pub use style::{Align, Color, Direction, Justify, Length, Style};

use canvas::CanvasContent;
use egui::Rect;
use state::Scope;
use std::{
    collections::{HashMap, HashSet},
    hash::{Hash, Hasher},
    sync::atomic::{AtomicU64, Ordering},
};
use taffy::{AvailableSpace as TaffyAvailableSpace, Size, TaffyTree};

type Callback<'a> = Box<dyn FnOnce() + 'a>;
type ScopePath = Vec<u64>;

struct FrameNode<'a> {
    id: egui::Id,
    parent: Option<usize>,
    children: Vec<usize>,
    style: Style,
    canvas: Option<CanvasContent<'a>>,
    events: Events<'a>,
    clickable: bool,
    focusable: bool,
}
struct Build<'a> {
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
    /// Shorthand for adding a container frame in the current state scope.
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
    /// Add a frame, constructing its children now, before layout or interaction.
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
    pub fn state<T: 'static>(&mut self, key: impl Hash, init: impl FnOnce() -> T) -> State<T> {
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
            parent: Some(self.parent),
            children: Vec::new(),
            style: frame.style,
            canvas: frame.canvas,
            events: frame.events,
            clickable: frame.clickable,
            focusable: frame.focusable,
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

/// Persistent state storage and backend integration for one tree.
/// The frame tree and its callbacks are discarded after every call to `show`.
pub struct Dgui {
    id: u64,
    next_mount: u64,
    scopes: HashMap<ScopePath, Scope>,
    layout: TaffyTree<usize>,
    #[cfg(test)]
    last_layout: Vec<(egui::Id, Rect)>,
}
impl Default for Dgui {
    fn default() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            next_mount: 0,
            scopes: HashMap::from([(Vec::new(), Scope::new(0))]),
            layout: TaffyTree::new(),
            #[cfg(test)]
            last_layout: Vec::new(),
        }
    }
}
impl Dgui {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build, lay out, draw, dispatch callbacks, then unmount absent scopes.
    /// The root fills the host's available rectangle. Use a finite host rectangle.
    /// Configure `egui::Options::max_passes` to one to avoid replaying callbacks.
    pub fn show<'frame>(&mut self, host: &mut egui::Ui, build: impl FnOnce(&mut Ui<'_, 'frame>)) {
        assert_eq!(
            host.ctx().options(|o| o.max_passes.get()),
            1,
            "dgui requires egui Options::max_passes = 1"
        );
        let root_rect = host.available_rect_before_wrap();
        assert!(
            root_rect.is_finite(),
            "dgui requires a finite host rectangle"
        );
        let mut tree = Build {
            nodes: vec![FrameNode {
                id: egui::Id::new((self.id, "root")),
                parent: None,
                children: Vec::new(),
                style: Style::column()
                    .width(root_rect.width())
                    .height(root_rect.height()),
                canvas: None,
                events: Events::default(),
                clickable: false,
                focusable: false,
            }],
            visited: HashSet::from([Vec::new()]),
            ordinals: HashMap::new(),
        };
        build(&mut Ui {
            build: &mut tree,
            scopes: &mut self.scopes,
            next_mount: &mut self.next_mount,
            runtime_id: self.id,
            path: Vec::new(),
            parent: 0,
        });

        self.layout.clear();
        let ids: Vec<_> = tree
            .nodes
            .iter()
            .enumerate()
            .map(|(index, node)| {
                self.layout
                    .new_leaf_with_context(node.style.taffy(), index)
                    .unwrap()
            })
            .collect();
        for (index, node) in tree.nodes.iter().enumerate() {
            let children: Vec<_> = node.children.iter().map(|&child| ids[child]).collect();
            self.layout.set_children(ids[index], &children).unwrap();
        }
        self.layout
            .compute_layout_with_measure(
                ids[0],
                Size {
                    width: TaffyAvailableSpace::Definite(root_rect.width()),
                    height: TaffyAvailableSpace::Definite(root_rect.height()),
                },
                |known, available, _, context, _| {
                    let node = &tree.nodes[context.copied().unwrap()];
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
                        .map_or([0.0; 2], |canvas| (canvas.measure)(host.ctx(), input));
                    Size {
                        width: known.width.unwrap_or(natural[0]),
                        height: known.height.unwrap_or(natural[1]),
                    }
                },
            )
            .unwrap();

        let mut geometry: Vec<(Rect, Rect)> = Vec::with_capacity(tree.nodes.len());
        let mut callbacks: Vec<Callback<'frame>> = Vec::new();
        #[cfg(test)]
        self.last_layout.clear();
        for (index, node) in tree.nodes.iter_mut().enumerate() {
            let layout = self.layout.layout(ids[index]).unwrap();
            let (origin, parent_clip) = node
                .parent
                .map_or((root_rect.min, host.clip_rect()), |parent| {
                    (geometry[parent].0.min, geometry[parent].1)
                });
            let rect = Rect::from_min_size(
                origin + egui::vec2(layout.location.x, layout.location.y),
                egui::vec2(layout.size.width, layout.size.height),
            );
            let clip = parent_clip.intersect(rect);
            geometry.push((rect, clip));
            #[cfg(test)]
            self.last_layout.push((node.id, rect));
            let content_rect = Rect::from_min_max(
                rect.min
                    + egui::vec2(
                        layout.padding.left + layout.border.left,
                        layout.padding.top + layout.border.top,
                    ),
                (rect.max
                    - egui::vec2(
                        layout.padding.right + layout.border.right,
                        layout.padding.bottom + layout.border.bottom,
                    ))
                .max(
                    rect.min
                        + egui::vec2(
                            layout.padding.left + layout.border.left,
                            layout.padding.top + layout.border.top,
                        ),
                ),
            );
            render(host, node, rect, content_rect, clip, &mut callbacks);
        }
        host.advance_cursor_after_rect(root_rect);
        for callback in callbacks {
            callback();
        }
        self.scopes.retain(|path, _| tree.visited.contains(path));
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
    let mut sense = egui::Sense::hover();
    if node.events.click.is_some() || node.clickable || node.focusable {
        sense |= egui::Sense::CLICK;
    }
    if node.focusable {
        sense |= egui::Sense::FOCUSABLE;
    }
    let response = ui.interact(rect, node.id.with("frame"), sense);
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
    for (triggered, callback) in [
        (clicked, &mut node.events.click),
        (hovered, &mut node.events.hover),
        (gained_focus, &mut node.events.focus),
        (lost_focus, &mut node.events.blur),
    ] {
        if triggered && let Some(callback) = callback.take() {
            callbacks.push(callback);
        }
    }
}

#[cfg(test)]
mod tests;
