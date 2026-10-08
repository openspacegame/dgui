//! Rendering traversal keeps native scroll containers nested so egui can route
//! wheel input from the innermost viewport to its ancestors at scroll limits.

use ahash::AHashSet as HashSet;

use crate::{Align, Callback, Direction, FrameNode, Justify, Length, Overflow, Style, render};
use egui::{Pos2, Rect};

/// Separate viewport sizing from intrinsic content sizing within a scroll frame.
pub fn content_style(viewport: &mut Style) -> Option<Style> {
    if viewport.overflow_x == Overflow::Hidden && viewport.overflow_y == Overflow::Hidden {
        return None;
    }
    let mut content = Style::default()
        .direction(viewport.direction)
        .gap(viewport.gap)
        .wrap(viewport.wrap)
        .align(viewport.align)
        .justify(viewport.justify)
        .shrink(0.0);
    if viewport.overflow_x == Overflow::Hidden {
        content.width = Length::Percent(1.0);
    }
    viewport.direction = Direction::Column;
    viewport.gap = 0.0;
    viewport.justify = Justify::Start;
    viewport.align = Align::Start;
    Some(content)
}

pub struct Drawing<'a, 'frame> {
    pub nodes: &'a mut [FrameNode<'frame>],
    pub layouts: Vec<taffy::Layout>,
    pub layout: &'a mut taffy::TaffyTree<usize>,
    pub ids: &'a [taffy::NodeId],
    pub callbacks: &'a mut Vec<Callback<'frame>>,
    pub dispatched: &'a mut HashSet<egui::Id>,
    #[cfg(test)]
    pub rectangles: Vec<(egui::Id, Rect)>,
}

impl Drawing<'_, '_> {
    pub fn draw(
        &mut self,
        host: &mut egui::Ui,
        index: usize,
        origin: Pos2,
        parent_clip: Rect,
    ) -> Rect {
        let layout = self.layouts[index];
        let rect = Rect::from_min_size(
            origin + egui::vec2(layout.location.x, layout.location.y),
            egui::vec2(layout.size.width, layout.size.height),
        );
        let clip = parent_clip.intersect(rect);
        let inset = egui::vec2(
            layout.padding.left + layout.border.left,
            layout.padding.top + layout.border.top,
        );
        let content_min = rect.min + inset;
        let content_rect = Rect::from_min_max(
            content_min,
            (rect.max
                - egui::vec2(
                    layout.padding.right + layout.border.right,
                    layout.padding.bottom + layout.border.bottom,
                ))
            .max(content_min),
        );
        let node = &mut self.nodes[index];
        #[cfg(test)]
        self.rectangles.push((node.id, rect));
        render(
            host,
            node,
            rect,
            content_rect,
            clip,
            self.callbacks,
            self.dispatched,
        );

        let axes = [
            node.style.overflow_x == Overflow::Scroll,
            node.style.overflow_y == Overflow::Scroll,
        ];
        let children = node.children.clone();
        if axes.iter().any(|&enabled| enabled) && !children.is_empty() {
            let mut viewport = host.new_child(
                egui::UiBuilder::new()
                    .id_salt(node.id.with("viewport"))
                    .max_rect(content_rect),
            );
            viewport.set_clip_rect(clip.intersect(content_rect));
            if !node.enabled {
                viewport.disable();
            }
            egui::ScrollArea::new(axes)
                .id_salt(node.id.with("scroll"))
                .auto_shrink([false, false])
                .show(&mut viewport, |content| {
                    // Native scrollbars can occupy an animated gutter. Reflow
                    // against the actual viewport before drawing, so wrapping
                    // and percentage widths use the same bounds as clipping.
                    let origin = content.next_widget_position();
                    for child in children {
                        self.reflow(content, child, axes);
                        let child_rect = self.draw(content, child, origin, content.clip_rect());
                        content.expand_to_include_rect(child_rect);
                    }
                });
        } else {
            for child in children {
                self.draw(host, child, rect.min, clip.intersect(content_rect));
            }
        }
        rect
    }

    fn reflow(&mut self, host: &egui::Ui, root: usize, axes: [bool; 2]) {
        use taffy::{AvailableSpace, Size};

        let available = host.available_size_before_wrap();
        let space = |scrolling, size| {
            if scrolling {
                AvailableSpace::MaxContent
            } else {
                AvailableSpace::Definite(size)
            }
        };
        self.layout
            .compute_layout_with_measure(
                self.ids[root],
                Size {
                    width: space(axes[0], available.x),
                    height: space(axes[1], available.y),
                },
                |known, available, _, context, _| {
                    crate::measure(
                        &self.nodes[context.copied().unwrap()],
                        host.ctx(),
                        known,
                        available,
                    )
                },
            )
            .unwrap();
        let mut pending = vec![root];
        while let Some(index) = pending.pop() {
            self.layouts[index] = *self.layout.layout(self.ids[index]).unwrap();
            pending.extend_from_slice(&self.nodes[index].children);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{Dgui, Frame, Length, Overflow, Style};

    fn run<'frame>(gui: &mut Dgui, style: Style, build: impl FnOnce(&mut crate::Ui<'_, 'frame>)) {
        let context = egui::Context::default();
        context.options_mut(|options| options.max_passes = 1.try_into().unwrap());
        let mut build = Some(build);
        context
            .run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(400.0, 300.0),
                    )),
                    ..Default::default()
                },
                |host| gui.show_styled(host, style.clone(), build.take().unwrap()),
            )
            .drop_without_applying_deltas();
    }

    #[test]
    fn automatic_root_height_follows_content_and_padding() {
        let mut gui = Dgui::new();
        run(
            &mut gui,
            Style::column()
                .width(Length::Percent(1.0))
                .padding(8.0)
                .gap(5.0),
            |ui| {
                ui.add(Frame::new().height(30.0));
                ui.add(Frame::new().height(40.0));
            },
        );
        assert_eq!(gui.last_layout[0].1.height(), 91.0);
    }

    #[test]
    fn scroll_viewport_bounds_natural_content_without_shrinking_rows() {
        let mut gui = Dgui::new();
        run(
            &mut gui,
            Style::column().width(Length::Percent(1.0)),
            |ui| {
                ui.frame(
                    Style::column().height(100.0).overflow_y(Overflow::Scroll),
                    |ui| {
                        for _ in 0..20 {
                            ui.add(Frame::new().height(30.0));
                        }
                    },
                );
            },
        );
        assert_eq!(gui.last_layout[0].1.height(), 100.0);
        assert_eq!(gui.last_layout[1].1.height(), 100.0);
        assert_eq!(gui.last_layout[2].1.height(), 600.0);
        for (_, rect) in &gui.last_layout[3..] {
            assert_eq!(rect.height(), 30.0);
        }
    }

    #[test]
    fn virtual_frame_builds_only_visible_rows() {
        let mut gui = Dgui::new();
        let count = std::cell::Cell::new(0);
        run(
            &mut gui,
            Style::column().width(Length::Percent(1.0)).height(100.0),
            |ui| {
                ui.add(Frame::virtual_list(
                    1_000_000,
                    20.0,
                    |index| index,
                    |ui, _| {
                        count.set(count.get() + 1);
                        ui.add(Frame::new().height(20.0));
                    },
                ));
            },
        );
        assert!(
            count.get() > 0 && count.get() <= 9,
            "built {} rows",
            count.get()
        );
    }

    #[test]
    fn horizontal_scroll_keeps_natural_row_width() {
        let mut gui = Dgui::new();
        run(&mut gui, Style::column().width(100.0).height(100.0), |ui| {
            ui.frame(
                Style::row()
                    .width(100.0)
                    .height(100.0)
                    .overflow_x(Overflow::Scroll),
                |ui| {
                    for _ in 0..20 {
                        ui.add(Frame::new().width(30.0).height(20.0));
                    }
                },
            );
        });
        assert_eq!(gui.last_layout[1].1.width(), 100.0);
        assert_eq!(gui.last_layout[2].1.width(), 600.0);
    }

    #[test]
    fn nested_scroll_content_uses_actual_native_viewport_width() {
        let context = egui::Context::default();
        context.options_mut(|options| options.max_passes = 1.try_into().unwrap());
        context.all_styles_mut(|style| {
            style.spacing.scroll = egui::style::ScrollStyle::solid();
            style.animation_time = 0.0;
        });
        let mut gui = Dgui::new();
        let native_width = std::cell::Cell::new(0.0);
        let nested_width = std::cell::Cell::new(0.0);
        for _ in 0..3 {
            context
                .run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(500.0, 400.0),
                        )),
                        ..Default::default()
                    },
                    |host| {
                        let mut reference =
                            host.new_child(egui::UiBuilder::new().id_salt("reference").max_rect(
                                egui::Rect::from_min_size(
                                    egui::pos2(250.0, 0.0),
                                    egui::vec2(200.0, 100.0),
                                ),
                            ));
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(&mut reference, |outer| {
                                let mut inner = outer.new_child(
                                    egui::UiBuilder::new().id_salt("inner").max_rect(
                                        egui::Rect::from_min_size(
                                            outer.next_widget_position(),
                                            egui::vec2(outer.available_width(), 60.0),
                                        ),
                                    ),
                                );
                                egui::ScrollArea::vertical()
                                    .auto_shrink([false, false])
                                    .show(&mut inner, |content| {
                                        native_width.set(content.available_width());
                                        content.set_min_height(600.0);
                                    });
                                outer.set_min_height(300.0);
                            });
                        gui.show_styled(
                            host,
                            Style::column()
                                .width(200.0)
                                .height(100.0)
                                .overflow_y(Overflow::Scroll),
                            |ui| {
                                ui.frame(
                                    Style::column().height(60.0).overflow_y(Overflow::Scroll),
                                    |ui| {
                                        ui.add(Frame::canvas(
                                            |_, input| [input.width(), 600.0],
                                            |canvas| {
                                                nested_width.set(canvas.content_rect.width());
                                            },
                                        ));
                                    },
                                );
                                ui.add(Frame::new().height(240.0));
                            },
                        );
                    },
                )
                .drop_without_applying_deltas();
        }
        assert!(native_width.get() < 200.0);
        assert_eq!(nested_width.get(), native_width.get());
    }

    #[test]
    fn nested_wheel_scroll_is_consumed_by_inner_viewport() {
        let context = egui::Context::default();
        context.options_mut(|options| options.max_passes = 1.try_into().unwrap());
        let mut gui = Dgui::new();
        let mut row_positions = Vec::new();
        for frame in 0..4 {
            let mut events = vec![egui::Event::PointerMoved(egui::pos2(20.0, 20.0))];
            if frame == 1 {
                events.push(egui::Event::MouseWheel {
                    phase: egui::TouchPhase::Move,
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -60.0),
                    modifiers: Default::default(),
                });
            }
            context
                .run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(200.0, 200.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |host| {
                        gui.show(host, |ui| {
                            ui.frame(
                                Style::column()
                                    .width(100.0)
                                    .height(100.0)
                                    .overflow_y(Overflow::Scroll),
                                |ui| {
                                    ui.frame(
                                        Style::column().height(60.0).overflow_y(Overflow::Scroll),
                                        |ui| {
                                            for _ in 0..20 {
                                                ui.add(Frame::new().height(20.0));
                                            }
                                        },
                                    );
                                    ui.add(Frame::new().height(200.0));
                                },
                            );
                        })
                    },
                )
                .drop_without_applying_deltas();
            row_positions.push((gui.last_layout[3].1.min.y, gui.last_layout[5].1.min.y));
        }
        assert_eq!(
            row_positions[0].0, row_positions[3].0,
            "outer viewport moved"
        );
        assert!(
            row_positions[3].1 < row_positions[0].1,
            "inner content did not scroll"
        );
    }
}
