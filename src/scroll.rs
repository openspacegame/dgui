//! Rendering traversal keeps native scroll containers nested so egui can route
//! wheel input from the innermost viewport to its ancestors at scroll limits.

use crate::{Callback, FrameNode, Overflow, render};
use egui::{Pos2, Rect};

/// Clip only the axes that establish an overflow boundary.
pub fn content_clip(parent: Rect, padding_box: Rect, overflow: [Overflow; 2]) -> Rect {
    let mut clip = parent;
    if overflow[0] != Overflow::Visible {
        clip.min.x = clip.min.x.max(padding_box.min.x);
        clip.max.x = clip.max.x.min(padding_box.max.x);
    }
    if overflow[1] != Overflow::Visible {
        clip.min.y = clip.min.y.max(padding_box.min.y);
        clip.max.y = clip.max.y.min(padding_box.max.y);
    }
    clip
}

fn scroll_area(id: egui::Id, axes: [bool; 2], visibility: Overflow) -> egui::ScrollArea {
    use egui::scroll_area::{ScrollBarVisibility, ScrollSource};
    egui::ScrollArea::new(axes)
        .id_salt(id)
        .auto_shrink([false, false])
        .scroll_bar_visibility(match visibility {
            Overflow::Scroll => ScrollBarVisibility::AlwaysVisible,
            Overflow::Hidden => ScrollBarVisibility::AlwaysHidden,
            _ => ScrollBarVisibility::VisibleWhenNeeded,
        })
        .scroll_source(if visibility == Overflow::Hidden {
            ScrollSource::NONE
        } else {
            ScrollSource::default()
        })
}

pub struct Drawing<'a, 'frame> {
    pub nodes: &'a mut [FrameNode<'frame>],
    pub layouts: Vec<taffy::Layout>,
    pub layout: &'a mut taffy::TaffyTree<usize>,
    pub ids: &'a [taffy::NodeId],
    pub callbacks: &'a mut Vec<Callback<'frame>>,
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
        let overflow = node.style.effective_overflow();
        let padding_box = Rect::from_min_max(
            rect.min + egui::vec2(layout.border.left, layout.border.top),
            rect.max - egui::vec2(layout.border.right, layout.border.bottom),
        );
        let clip = content_clip(parent_clip, padding_box, overflow);
        let axes = overflow.map(Overflow::user_scroll);
        if !axes.iter().any(|&enabled| enabled) {
            render(
                host,
                node,
                rect,
                content_rect,
                parent_clip,
                clip,
                self.callbacks,
            );
        }
        let children = node.children.clone();
        if axes.iter().any(|&enabled| enabled) {
            let id = node.id;
            let enabled = node.enabled;
            let mut viewport = host.new_child(
                egui::UiBuilder::new()
                    .id_salt(id.with("viewport"))
                    .max_rect(content_rect),
            );
            viewport.set_clip_rect(clip.intersect(content_rect));
            if !enabled {
                viewport.disable();
            }
            // Separate native areas preserve independently requested scrollbar
            // policies. Both receive the same finite viewport extent.
            if axes == [true, true] && overflow[0] != overflow[1] {
                scroll_area(id.with("scroll-x"), [true, false], overflow[0]).show_viewport(
                    &mut viewport,
                    |horizontal, x_view| {
                        horizontal.set_width(x_view.width());
                        scroll_area(id.with("scroll-y"), [false, true], overflow[1])
                            .max_height(content_rect.height())
                            .show_viewport(horizontal, |content, y_view| {
                                self.draw_scrolled(
                                    content,
                                    index,
                                    y_view.size(),
                                    inset,
                                    rect,
                                    parent_clip,
                                );
                            });
                    },
                );
            } else {
                let visibility = if axes[0] { overflow[0] } else { overflow[1] };
                scroll_area(id.with("scroll"), axes, visibility).show_viewport(
                    &mut viewport,
                    |content, view| {
                        self.draw_scrolled(content, index, view.size(), inset, rect, parent_clip);
                    },
                );
            }
        } else {
            for child in children {
                self.draw(host, child, rect.min, clip);
            }
        }
        rect
    }

    fn draw_scrolled(
        &mut self,
        content: &mut egui::Ui,
        index: usize,
        viewport: egui::Vec2,
        inset: egui::Vec2,
        frame_rect: Rect,
        parent_clip: Rect,
    ) {
        let original = self.layouts[index];
        let size = egui::vec2(
            viewport.x
                + original.padding.left
                + original.padding.right
                + original.border.left
                + original.border.right,
            viewport.y
                + original.padding.top
                + original.padding.bottom
                + original.border.top
                + original.border.bottom,
        );
        self.reflow(content.ctx(), index, size);
        let layout = self.layouts[index];
        let content_origin = content.next_widget_position();
        let origin = content_origin - inset;
        let node = &mut self.nodes[index];
        let measured = crate::measure(
            node,
            content.ctx(),
            taffy::Size::NONE,
            taffy::Size {
                width: taffy::AvailableSpace::Definite(viewport.x),
                height: taffy::AvailableSpace::Definite(viewport.y),
            },
        );
        let extent = viewport.max(egui::vec2(measured.width, measured.height));
        render(
            content,
            node,
            frame_rect,
            Rect::from_min_size(content_origin, extent),
            parent_clip,
            content.clip_rect(),
            self.callbacks,
        );
        for child in self.nodes[index].children.clone() {
            self.draw(content, child, origin, content.clip_rect());
        }
        // Taffy's overflow extent includes visible overflow from descendants
        // and stops at nested clipping/scrolling boundaries.
        content.set_min_size(egui::vec2(
            (layout.content_size.width - inset.x).max(extent.x),
            (layout.content_size.height - inset.y).max(extent.y),
        ));
    }

    fn reflow(&mut self, context: &egui::Context, root: usize, size: egui::Vec2) {
        use taffy::{AvailableSpace, Dimension, Size};
        let id = self.ids[root];
        let mut style = self.layout.style(id).unwrap().clone();
        // Preserve the parent's allocated border box while accounting for the
        // actual native scrollbar gutter. Do not introduce a flex wrapper.
        style.box_sizing = taffy::BoxSizing::BorderBox;
        style.size = Size {
            width: Dimension::length(size.x),
            height: Dimension::length(size.y),
        };
        style.min_size = style.size;
        style.max_size = style.size;
        self.layout.set_style(id, style).unwrap();
        crate::compute_layout(
            self.layout,
            id,
            Size {
                width: AvailableSpace::Definite(size.x),
                height: AvailableSpace::Definite(size.y),
            },
            self.nodes,
            context,
            None,
        );
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
                ui.add(Frame::new().height(30.0).shrink(0.0));
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
                    Style::column().height(100.0).overflow_y(Overflow::Auto),
                    |ui| {
                        for _ in 0..20 {
                            ui.add(Frame::new().height(30.0).shrink(0.0));
                        }
                    },
                );
            },
        );
        assert_eq!(gui.last_layout[0].1.height(), 100.0);
        assert_eq!(gui.last_layout[1].1.height(), 100.0);
        for (_, rect) in &gui.last_layout[2..] {
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
                ui.add(
                    crate::VirtualList::uniform(
                        1_000_000,
                        20.0,
                        |index| index,
                        |ui, _| {
                            count.set(count.get() + 1);
                            ui.add(Frame::new().height(20.0).shrink(0.0));
                        },
                    )
                    .frame(),
                );
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
                        ui.add(Frame::new().width(30.0).height(20.0).shrink(0.0));
                    }
                },
            );
        });
        assert_eq!(gui.last_layout[1].1.width(), 100.0);
        assert_eq!(gui.last_layout[21].1.right(), 600.0);
    }

    #[test]
    fn horizontal_scroll_fills_viewport_and_preserves_column_minimums() {
        for width in [100.0, 320.0] {
            let mut gui = Dgui::new();
            run(&mut gui, Style::column().width(width).height(100.0), |ui| {
                ui.frame(
                    Style::column()
                        .width(Length::Percent(1.0))
                        .height(100.0)
                        .overflow_x(Overflow::Scroll),
                    |ui| {
                        ui.frame(
                            Style::row()
                                .width(Length::Percent(1.0))
                                .min_width(200.0)
                                .height(24.0),
                            |ui| {
                                ui.add(Frame::new().width(100.0).shrink(0.0));
                                ui.add(Frame::new().min_width(100.0).grow(1.0));
                            },
                        );
                    },
                );
            });
            assert_eq!(gui.last_layout[2].1.width(), width.max(200.0));
        }
    }

    #[test]
    fn horizontal_scroll_preserves_height_for_nested_virtual_list() {
        let mut gui = Dgui::new();
        let visible_rows = std::cell::Cell::new(0);
        run(&mut gui, Style::column().width(200.0).height(300.0), |ui| {
            ui.frame(
                Style::column()
                    .grow(1.0)
                    .overflow_x(Overflow::Auto)
                    .overflow_y(Overflow::Hidden),
                |ui| {
                    ui.add(Frame::new().width(600.0).height(32.0).shrink(0.0));
                    ui.add(
                        crate::VirtualList::uniform(
                            20,
                            32.0,
                            |index| index,
                            |ui, _| {
                                ui.add(Frame::canvas(
                                    |_, input| [input.width(), 32.0],
                                    |canvas| {
                                        if canvas
                                            .content_rect
                                            .intersect(canvas.ui.clip_rect())
                                            .is_positive()
                                        {
                                            visible_rows.set(visible_rows.get() + 1);
                                        }
                                    },
                                ));
                            },
                        )
                        .frame(),
                    );
                },
            );
        });
        assert!(
            visible_rows.get() >= 7,
            "only {} rows are visible",
            visible_rows.get()
        );
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
                                .overflow_y(Overflow::Auto),
                            |ui| {
                                ui.frame(
                                    Style::column()
                                        .height(60.0)
                                        .shrink(0.0)
                                        .overflow_y(Overflow::Auto),
                                    |ui| {
                                        ui.add(Frame::canvas(
                                            |_, input| [input.width(), 600.0],
                                            |canvas| {
                                                nested_width.set(canvas.content_rect.width());
                                            },
                                        ));
                                    },
                                );
                                ui.add(Frame::new().height(240.0).shrink(0.0));
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
                                    .overflow_y(Overflow::Auto),
                                |ui| {
                                    ui.frame(
                                        Style::column()
                                            .height(60.0)
                                            .shrink(0.0)
                                            .overflow_y(Overflow::Auto),
                                        |ui| {
                                            for _ in 0..20 {
                                                ui.add(Frame::new().height(20.0).shrink(0.0));
                                            }
                                        },
                                    );
                                    ui.add(Frame::new().height(200.0).shrink(0.0));
                                },
                            );
                        })
                    },
                )
                .drop_without_applying_deltas();
            row_positions.push((gui.last_layout[2].1.min.y, gui.last_layout[3].1.min.y));
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

#[cfg(test)]
mod policy_tests {
    use crate::{Overflow, Style};

    #[test]
    fn overflow_axes_normalize_together() {
        assert_eq!(
            Style::default()
                .overflow_y(Overflow::Auto)
                .effective_overflow(),
            [Overflow::Auto; 2]
        );
        assert_eq!(
            Style::default()
                .overflow_x(Overflow::Clip)
                .overflow_y(Overflow::Scroll)
                .effective_overflow(),
            [Overflow::Hidden, Overflow::Scroll]
        );
        assert_eq!(
            Style::default()
                .overflow_x(Overflow::Clip)
                .effective_overflow(),
            [Overflow::Clip, Overflow::Visible]
        );
    }
}
