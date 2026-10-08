use super::*;
use std::{
    cell::Cell,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

fn context() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.options_mut(|options| options.max_passes = 1.try_into().unwrap());
    ctx
}
fn run<'a>(
    ctx: &egui::Context,
    gui: &mut Dgui,
    size: [f32; 2],
    events: Vec<egui::Event>,
    build: impl FnOnce(&mut Ui<'_, 'a>),
) {
    let mut build = Some(build);
    ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(size[0], size[1]),
            )),
            events,
            ..Default::default()
        },
        |host| gui.show(host, build.take().unwrap()),
    )
    .drop_without_applying_deltas();
}
fn pointer(pos: egui::Pos2, pressed: bool) -> Vec<egui::Event> {
    vec![
        egui::Event::PointerMoved(pos),
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        },
    ]
}
fn near(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 1.1, "{actual} != {expected}");
}

#[test]
fn runtime_and_state_are_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Dgui>();
    assert_send_sync::<State<String>>();
}

#[test]
fn state_updates_are_synchronized_across_threads() {
    let ctx = context();
    let mut gui = Dgui::new();
    let mut handle = None;
    run(&ctx, &mut gui, [100.0, 100.0], vec![], |ui| {
        handle = Some(ui.state("counter", || 0));
    });
    let handle = handle.unwrap();
    std::thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(move || {
                for _ in 0..100 {
                    handle.update(|value| *value += 1);
                }
            });
        }
    });
    assert_eq!(handle.get(), 400);
}

#[test]
fn repeated_passes_dispatch_effects_once_and_accept_later_effects() {
    let ctx = egui::Context::default();
    ctx.options_mut(|options| options.max_passes = 3.try_into().unwrap());
    let mut gui = Dgui::new();
    let first = Cell::new(0);
    let later = Cell::new(0);
    let passes = Cell::new(0);
    ctx.run_ui(egui::RawInput::default(), |host| {
        let pass = host.ctx().current_pass_index();
        passes.set(passes.get() + 1);
        gui.show(host, |ui| {
            ui.add(Frame::canvas(
                |_, _| [20.0, 20.0],
                |canvas| {
                    canvas.defer_keyed("first", || first.set(first.get() + 1));
                    if pass > 0 {
                        canvas.defer_keyed("later", || later.set(later.get() + 1));
                    }
                },
            ));
        });
        if pass < 2 {
            host.ctx().request_discard("exercise runtime replay");
        }
    })
    .drop_without_applying_deltas();
    assert_eq!(passes.get(), 3);
    assert_eq!(first.get(), 1);
    assert_eq!(later.get(), 1);
}

#[test]
fn repeated_passes_dispatch_pointer_click_once() {
    let ctx = egui::Context::default();
    ctx.options_mut(|options| options.max_passes = 2.try_into().unwrap());
    let mut gui = Dgui::new();
    let clicks = Cell::new(0);
    let responses = Cell::new(0);
    for events in [
        vec![],
        pointer(egui::pos2(10.0, 10.0), true),
        pointer(egui::pos2(10.0, 10.0), false),
    ] {
        ctx.run_ui(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |host| {
                gui.show(host, |ui| {
                    ui.add(
                        button("click")
                            .on_click(|| clicks.set(clicks.get() + 1))
                            .on_response(|response| {
                                if response.clicked() {
                                    responses.set(responses.get() + 1);
                                }
                            }),
                    );
                });
                if host.ctx().current_pass_index() == 0 {
                    host.ctx().request_discard("replay click");
                }
            },
        )
        .drop_without_applying_deltas();
    }
    assert_eq!(clicks.get(), 1);
    assert_eq!(responses.get(), 1);
}

#[test]
fn scope_absent_in_first_pass_survives_later_pass() {
    let ctx = context();
    let mut gui = Dgui::new();
    let mut handle = None;
    run(&ctx, &mut gui, [400.0, 300.0], vec![], |ui| {
        ui.scope("conditional", |ui| handle = Some(ui.state("value", || 41)));
    });
    ctx.options_mut(|options| options.max_passes = 2.try_into().unwrap());
    ctx.run_ui(egui::RawInput::default(), |host| {
        let pass = host.ctx().current_pass_index();
        gui.show(host, |ui| {
            if pass == 1 {
                ui.scope("conditional", |ui| {
                    assert_eq!(ui.state("value", || 0).get(), 41);
                });
            }
        });
        if pass == 0 {
            host.ctx().request_discard("discover conditional content");
        }
    })
    .drop_without_applying_deltas();
    assert_eq!(handle.unwrap().get(), 41);
}

#[test]
fn disabled_container_disables_native_and_composed_descendants() {
    let ctx = context();
    let mut gui = Dgui::new();
    let clicks = Cell::new(0);
    for events in [
        vec![],
        pointer(egui::pos2(10.0, 10.0), true),
        pointer(egui::pos2(10.0, 10.0), false),
    ] {
        run(&ctx, &mut gui, [400.0, 300.0], events, |ui| {
            ui.add(Frame::new().enabled(false).children(|ui| {
                ui.add(button("disabled").on_click(|| clicks.set(clicks.get() + 1)));
                ui.add(Frame::egui_canvas([100.0, 30.0], |ui| {
                    assert!(!ui.is_enabled())
                }));
            }));
        });
    }
    assert_eq!(clicks.get(), 0);
}

#[test]
fn state_is_copy_even_for_non_copy_values_and_initializes_once() {
    let ctx = context();
    let mut gui = Dgui::new();
    let calls = Cell::new(0);
    let mut handle = None;
    for _ in 0..3 {
        run(&ctx, &mut gui, [400.0, 300.0], vec![], |ui| {
            let state = ui.state("name", || {
                calls.set(calls.get() + 1);
                String::from("A")
            });
            let copy = state;
            copy.update(|name| name.push('!'));
            assert_eq!(copy.get(), state.get());
            handle = Some(state);
        });
    }
    assert_eq!(calls.get(), 1);
    assert_eq!(handle.unwrap().get(), "A!!!");
}

#[test]
fn keyed_scopes_survive_reordering_and_release_values_on_unmount() {
    struct DropCounter(Arc<AtomicUsize>);
    impl Drop for DropCounter {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }
    let ctx = context();
    let mut gui = Dgui::new();
    let drops = Arc::new(AtomicUsize::new(0));
    let mut handles = HashMap::new();
    for keys in [[1, 2], [2, 1]] {
        run(&ctx, &mut gui, [400.0, 300.0], vec![], |ui| {
            for key in keys {
                ui.scope(key, |ui| {
                    let value = ui.state("value", || key * 10);
                    ui.state("drop", || DropCounter(drops.clone()));
                    value.update(|value| *value += 1);
                    handles.insert(key, value);
                });
            }
        });
    }
    assert_eq!(handles[&1].get(), 12);
    assert_eq!(handles[&2].get(), 22);
    assert_eq!(drops.load(Ordering::Relaxed), 0);
    let stale = handles[&2];
    run(&ctx, &mut gui, [400.0, 300.0], vec![], |ui| {
        ui.scope(1, |_| {})
    });
    // Unmount waits for the frame boundary so subsequent passes can revisit it.
    run(&ctx, &mut gui, [400.0, 300.0], vec![], |ui| {
        ui.scope(1, |_| {})
    });
    assert_eq!(drops.load(Ordering::Relaxed), 1);
    assert!(stale.0.try_read().is_err());
    assert_eq!(handles[&1].get(), 12);
    run(&ctx, &mut gui, [400.0, 300.0], vec![], |ui| {
        ui.scope(1, |_| {});
        ui.scope(2, |ui| assert_eq!(ui.state("value", || 20).get(), 20));
    });
    assert!(stale.0.try_read().is_err());
    drop(gui);
    assert_eq!(drops.load(Ordering::Relaxed), 2);
    assert!(handles[&1].0.try_read().is_err());
}

#[test]
fn runtimes_are_independent_and_unrelated_states_can_be_borrowed_together() {
    let ctx = context();
    let mut first = Dgui::new();
    let mut second = Dgui::new();
    let mut handle = None;
    run(&ctx, &mut first, [400.0, 300.0], vec![], |ui| {
        let a = ui.state("a", || 1);
        let b = ui.state("b", || 2);
        a.update(|value| *value += b.get());
        assert_eq!(a.get(), 3);
        assert!(catch_unwind(AssertUnwindSafe(|| a.update(|_| a.set(5)))).is_err());
        handle = Some(a);
    });
    run(&ctx, &mut second, [400.0, 300.0], vec![], |ui| {
        assert_eq!(ui.state("a", || 100).get(), 100);
    });
    assert_eq!(handle.unwrap().get(), 3);
}

#[test]
#[should_panic(expected = "duplicate sibling scope key")]
fn duplicate_scope_keys_fail_clearly() {
    run(&context(), &mut Dgui::new(), [400.0, 300.0], vec![], |ui| {
        ui.scope("same", |_| {});
        ui.scope("same", |_| {});
    });
}

#[test]
#[should_panic(expected = "different type")]
fn a_state_key_cannot_change_type() {
    run(&context(), &mut Dgui::new(), [400.0, 300.0], vec![], |ui| {
        ui.state("same", || 1);
        ui.state("same", String::new);
    });
}

#[test]
fn flex_grow_padding_and_leaf_constraints_share_the_frame_model() {
    let mut gui = Dgui::new();
    run(&context(), &mut gui, [400.0, 300.0], vec![], |ui| {
        ui.frame(Style::row().width(300.0).padding(10.0).gap(10.0), |ui| {
            ui.add(
                Frame::text("fixed").style(Style::default().width(80.0).shrink(0.0).padding(5.0)),
            );
            ui.add(button("grow").style(Style::default().width(0.0).grow(1.0).padding(5.0)));
        });
    });
    let container = gui.last_layout[1].1;
    let fixed = gui.last_layout[2].1;
    let grow = gui.last_layout[3].1;
    near(container.width(), 300.0);
    near(fixed.left(), container.left() + 10.0);
    near(fixed.width(), 80.0);
    near(grow.left(), fixed.right() + 10.0);
    near(grow.width(), 190.0);
    near(grow.right(), container.right() - 10.0);
}

#[test]
fn text_and_flex_items_wrap_when_the_viewport_narrows() {
    let ctx = context();
    let mut gui = Dgui::new();
    let mut heights = vec![];
    for width in [600.0, 220.0] {
        run(&ctx, &mut gui, [width, 600.0], vec![], |ui| {
            ui.frame(Style::column().gap(10.0), |ui| {
                ui.add(Frame::text("A paragraph with enough words to require several lines in the narrower viewport."));
                ui.frame(Style::row().wrap(true).gap(10.0), |ui| {
                    for _ in 0..2 { ui.add(Frame::text("card").style(Style::default().width(150.0).shrink(0.0))); }
                });
            });
        });
        heights.push(gui.last_layout[2].1.height());
        if width > 300.0 {
            near(gui.last_layout[4].1.top(), gui.last_layout[5].1.top());
        } else {
            assert!(gui.last_layout[5].1.top() > gui.last_layout[4].1.bottom());
        }
    }
    assert!(heights[1] > heights[0]);
}

#[test]
fn canvas_runs_once_after_layout_with_padding_and_clipping() {
    let mut gui = Dgui::new();
    let calls = Cell::new(0);
    run(&context(), &mut gui, [400.0, 300.0], vec![], |ui| {
        assert_eq!(calls.get(), 0);
        ui.frame(
            Style::default().width(100.0).height(80.0).padding(10.0),
            |ui| {
                ui.add(
                    Frame::egui_canvas([150.0, 100.0], |canvas| {
                        calls.set(calls.get() + 1);
                        near(canvas.max_rect().width(), 140.0);
                        assert!(canvas.clip_rect().right() <= 100.0);
                        near(canvas.max_rect().left(), 15.0);
                    })
                    .style(
                        Style::default()
                            .width(150.0)
                            .height(100.0)
                            .padding(5.0)
                            .shrink(0.0),
                    ),
                );
            },
        );
        assert_eq!(calls.get(), 0);
    });
    assert_eq!(calls.get(), 1);
}

#[test]
fn click_dispatch_is_once_and_after_rendering_and_allows_borrowed_callbacks() {
    let ctx = context();
    let mut gui = Dgui::new();
    let mut external_calls = 0;
    let mut observed = Vec::new();
    let mut handle = None;
    let mut pos = egui::pos2(10.0, 10.0);
    for phase in 0..4 {
        let events = match phase {
            1 => pointer(pos, true),
            2 => pointer(pos, false),
            _ => vec![],
        };
        run(&ctx, &mut gui, [400.0, 300.0], events, |ui| {
            let count = ui.state("count", || 0);
            handle = Some(count);
            let external_calls = &mut external_calls;
            ui.add(button("increment").on_click(move || {
                *external_calls += 1;
                count.update(|n| *n += 1);
            }));
            let observed = &mut observed;
            ui.add(Frame::egui_canvas([20.0, 20.0], move |_| {
                observed.push(count.get())
            }));
        });
        pos = gui.last_layout[1].1.center();
    }
    assert_eq!(external_calls, 1);
    assert_eq!(handle.unwrap().get(), 1);
    assert_eq!(observed, [0, 0, 0, 1]);
}

#[test]
fn text_edit_focus_and_state_follow_keyed_scopes_and_reset_on_remount() {
    let ctx = context();
    let mut gui = Dgui::new();
    let mut states = HashMap::new();
    let build = |ui: &mut Ui<'_, '_>, keys: &[u32], states: &mut HashMap<u32, State<String>>| {
        for &key in keys {
            ui.scope(key, |ui| {
                let name = ui.state("name", String::new);
                states.insert(key, name);
                ui.add(text_input(name));
            });
        }
    };
    run(&ctx, &mut gui, [400.0, 300.0], vec![], |ui| {
        build(ui, &[1, 2], &mut states)
    });
    let (first_id, first_rect) = gui.last_layout[1];
    let second_id = gui.last_layout[2].0;
    for pressed in [true, false] {
        run(
            &ctx,
            &mut gui,
            [400.0, 300.0],
            pointer(first_rect.center(), pressed),
            |ui| build(ui, &[1, 2], &mut states),
        );
    }
    assert_eq!(ctx.memory(|mem| mem.focused()), Some(first_id));
    run(
        &ctx,
        &mut gui,
        [400.0, 300.0],
        vec![egui::Event::Text("Ada".into())],
        |ui| build(ui, &[1, 2], &mut states),
    );
    assert_eq!(states[&1].get(), "Ada");
    run(
        &ctx,
        &mut gui,
        [400.0, 300.0],
        vec![egui::Event::Text("!".into())],
        |ui| build(ui, &[2, 1], &mut states),
    );
    assert_eq!(gui.last_layout[1].0, second_id);
    assert_eq!(gui.last_layout[2].0, first_id);
    assert_eq!(ctx.memory(|mem| mem.focused()), Some(first_id));
    assert_eq!(states[&1].get(), "Ada!");
    assert_eq!(states[&2].get(), "");
    run(&ctx, &mut gui, [400.0, 300.0], vec![], |ui| {
        build(ui, &[2], &mut states)
    });
    run(&ctx, &mut gui, [400.0, 300.0], vec![], |ui| {
        build(ui, &[1, 2], &mut states)
    });
    assert_ne!(gui.last_layout[1].0, first_id);
    assert_eq!(states[&1].get(), "");
}

#[test]
fn canvas_button_interacts_in_padding_but_respects_ancestor_clipping() {
    let ctx = context();
    let mut gui = Dgui::new();
    let mut clicks = 0;
    let events = [
        vec![],
        pointer(egui::pos2(3.0, 3.0), true),
        pointer(egui::pos2(3.0, 3.0), false),
        pointer(egui::pos2(90.0, 10.0), true),
        pointer(egui::pos2(90.0, 10.0), false),
    ];
    for events in events {
        run(&ctx, &mut gui, [400.0, 300.0], events, |ui| {
            ui.frame(Style::column().width(60.0).height(50.0), |ui| {
                ui.add(
                    button("click")
                        .style(
                            Style::default()
                                .width(120.0)
                                .height(40.0)
                                .padding(10.0)
                                .shrink(0.0),
                        )
                        .on_click(|| clicks += 1),
                );
            });
        });
    }
    assert_eq!(clicks, 1);
}

#[test]
fn nested_frame_clicks_target_the_deepest_interactive_frame_and_exclude_margins() {
    let ctx = context();
    let mut gui = Dgui::new();
    let outer = Cell::new(0);
    let inner = Cell::new(0);
    let outer_ref = &outer;
    let inner_ref = &inner;
    let build = move || {
        Frame::new()
            .width(180.0)
            .height(100.0)
            .padding(12.0)
            .margin(10.0)
            .on_click(move || outer_ref.set(outer_ref.get() + 1))
            .children(move |ui| {
                ui.add(
                    button("child")
                        .width(80.0)
                        .height(40.0)
                        .on_click(move || inner_ref.set(inner_ref.get() + 1)),
                )
            })
    };
    run(&ctx, &mut gui, [400.0, 300.0], vec![], |ui| ui.add(build()));
    let child_pos = gui.last_layout[2].1.center();
    let padding_pos = gui.last_layout[1].1.min + egui::vec2(3.0, 3.0);
    for pos in [child_pos, padding_pos, egui::pos2(2.0, 2.0)] {
        for pressed in [true, false] {
            run(
                &ctx,
                &mut gui,
                [400.0, 300.0],
                pointer(pos, pressed),
                |ui| ui.add(build()),
            );
        }
    }
    assert_eq!(inner.get(), 1);
    assert_eq!(outer.get(), 1);
}

#[test]
fn all_frame_constructors_share_padding_border_margin_and_sizing() {
    let ctx = context();
    let mut gui = Dgui::new();
    let content = Cell::new(Rect::ZERO);
    run(&ctx, &mut gui, [400.0, 500.0], vec![], |ui| {
        let text = ui.state("text", || String::from("editor"));
        let frames: Vec<Frame<'_>> = vec![
            button("button"),
            text_input(text),
            Frame::canvas(
                |_, _| [10.0, 10.0],
                |canvas| content.set(canvas.content_rect),
            ),
        ];
        for frame in frames {
            ui.add(
                frame
                    .width(120.0)
                    .height(60.0)
                    .padding(8.0)
                    .margin(7.0)
                    .border(2.0, Color::rgb(80, 90, 100))
                    .shrink(0.0),
            );
        }
    });
    let rects: Vec<_> = [1, 3, 4].map(|index| gui.last_layout[index].1).into();
    for rect in &rects {
        near(rect.left(), 7.0);
        near(rect.width(), 120.0);
        near(rect.height(), 60.0);
    }
    near(rects[1].top() - rects[0].bottom(), 14.0);
    near(rects[2].top() - rects[1].bottom(), 14.0);
    near(content.get().left() - rects[2].left(), 10.0);
    near(content.get().width(), 100.0);
    near(content.get().height(), 40.0);
}

fn key(key: egui::Key) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Default::default(),
    }
}

#[test]
fn focusable_frames_activate_with_keyboard_and_pointer_only_frames_do_not_take_tab_focus() {
    let ctx = context();
    let mut gui = Dgui::new();
    let activations = Cell::new(0);
    let focused = Cell::new(0);
    let activations_ref = &activations;
    let focused_ref = &focused;
    let frames = move || {
        [
            Frame::text("pointer only").on_click(|| panic!("pointer-only frame activated")),
            button("keyboard")
                .on_click(move || activations_ref.set(activations_ref.get() + 1))
                .on_focus(move || focused_ref.set(focused_ref.get() + 1)),
        ]
    };
    run(&ctx, &mut gui, [400.0, 300.0], vec![], |ui| {
        for frame in frames() {
            ui.add(frame);
        }
    });
    let button_id = gui.last_layout[2].0.with("frame");
    run(
        &ctx,
        &mut gui,
        [400.0, 300.0],
        vec![key(egui::Key::Tab)],
        |ui| {
            for frame in frames() {
                ui.add(frame);
            }
        },
    );
    run(&ctx, &mut gui, [400.0, 300.0], vec![], |ui| {
        for frame in frames() {
            ui.add(frame);
        }
    });
    assert_eq!(ctx.memory(|m| m.focused()), Some(button_id));
    run(
        &ctx,
        &mut gui,
        [400.0, 300.0],
        vec![key(egui::Key::Enter)],
        |ui| {
            for frame in frames() {
                ui.add(frame);
            }
        },
    );
    run(
        &ctx,
        &mut gui,
        [400.0, 300.0],
        vec![key(egui::Key::Space)],
        |ui| {
            for frame in frames() {
                ui.add(frame);
            }
        },
    );
    assert_eq!(activations.get(), 2);
    assert_eq!(focused.get(), 1);
}

#[test]
fn editor_frame_handlers_cooperate_with_native_input_and_padding_focus() {
    let ctx = context();
    let mut gui = Dgui::new();
    let clicks = Cell::new(0);
    let parent_clicks = Cell::new(0);
    let focused = Cell::new(0);
    let blurred = Cell::new(0);
    let handle = Cell::new(None);
    fn build<'a>(
        ui: &mut Ui<'_, 'a>,
        clicks: &'a Cell<i32>,
        parent_clicks: &'a Cell<i32>,
        focused: &'a Cell<i32>,
        blurred: &'a Cell<i32>,
        handle: &Cell<Option<State<String>>>,
    ) {
        let state = ui.state("text", String::new);
        handle.set(Some(state));
        ui.add(
            Frame::new()
                .padding(10.0)
                .width(300.0)
                .height(130.0)
                .on_click(move || parent_clicks.set(parent_clicks.get() + 1))
                .children(move |ui| {
                    ui.add(
                        text_input(state)
                            .width(200.0)
                            .on_click(move || clicks.set(clicks.get() + 1))
                            .on_focus(move || focused.set(focused.get() + 1))
                            .on_blur(move || blurred.set(blurred.get() + 1)),
                    )
                }),
        );
    }
    // References survive in child-build closures, just like Copy state handles.
    run(&ctx, &mut gui, [400.0, 300.0], vec![], |ui| {
        build(ui, &clicks, &parent_clicks, &focused, &blurred, &handle)
    });
    let editor = gui.last_layout[2];
    let padding_pos = editor.1.min + egui::vec2(2.0, 2.0);
    for pressed in [true, false] {
        run(
            &ctx,
            &mut gui,
            [400.0, 300.0],
            pointer(padding_pos, pressed),
            |ui| build(ui, &clicks, &parent_clicks, &focused, &blurred, &handle),
        );
    }
    assert_eq!(ctx.memory(|m| m.focused()), Some(editor.0));
    assert_eq!(clicks.get(), 1);
    run(
        &ctx,
        &mut gui,
        [400.0, 300.0],
        vec![
            key(egui::Key::Space),
            egui::Event::Text("hello world".into()),
        ],
        |ui| build(ui, &clicks, &parent_clicks, &focused, &blurred, &handle),
    );
    assert_eq!(handle.get().unwrap().get(), "hello world");
    assert_eq!(clicks.get(), 1, "typing must not activate a frame click");
    for pressed in [true, false] {
        run(
            &ctx,
            &mut gui,
            [400.0, 300.0],
            pointer(editor.1.center(), pressed),
            |ui| build(ui, &clicks, &parent_clicks, &focused, &blurred, &handle),
        );
    }
    assert_eq!(clicks.get(), 2);
    assert_eq!(parent_clicks.get(), 0);
    for pressed in [true, false] {
        run(
            &ctx,
            &mut gui,
            [400.0, 300.0],
            pointer(egui::pos2(3.0, 3.0), pressed),
            |ui| build(ui, &clicks, &parent_clicks, &focused, &blurred, &handle),
        );
    }
    assert_eq!(parent_clicks.get(), 1);
    assert_eq!(focused.get(), 1);
    assert_eq!(blurred.get(), 1);
}
