//! This integration test can only access dgui's public extension API.
use dgui::{Dgui, Frame};
use std::cell::Cell;

struct ResponsivePanel<'a> {
    measured: &'a Cell<usize>,
    drawn: &'a Cell<usize>,
    effects: &'a Cell<usize>,
    final_size: &'a Cell<[f32; 2]>,
}
fn responsive_panel<'a>(panel: ResponsivePanel<'a>) -> Frame<'a> {
    Frame::new().padding(10.0).children(move |ui| {
        ui.add(Frame::text("Custom component"));
        let measured = panel.measured;
        let drawn = panel.drawn;
        ui.add(Frame::canvas(
            move |_, input| {
                assert_eq!(drawn.get(), 0, "all layout must precede drawing");
                measured.set(measured.get() + 1);
                let width = input.width().clamp(1.0, 200.0);
                [width, 2000.0 / width]
            },
            move |canvas| {
                assert!(panel.measured.get() > 0);
                assert_eq!(panel.effects.get(), 0);
                panel.drawn.set(panel.drawn.get() + 1);
                panel.final_size.set(canvas.content_rect.size().into());
                canvas.defer(move || panel.effects.set(panel.effects.get() + 1));
            },
        ));
    })
}

#[test]
fn component_function_composes_measured_content_and_defers_borrowed_effects() {
    let ctx = egui::Context::default();
    ctx.options_mut(|o| o.max_passes = 1.try_into().unwrap());
    let mut gui = Dgui::new();
    let mut sizes = Vec::new();
    for width in [300.0, 100.0] {
        let measured = Cell::new(0);
        let drawn = Cell::new(0);
        let effects = Cell::new(0);
        let final_size = Cell::new([0.0; 2]);
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width, 400.0),
                )),
                ..Default::default()
            },
            |host| {
                gui.show(host, |ui| {
                    ui.scope("panel", |ui| {
                        ui.add(responsive_panel(ResponsivePanel {
                            measured: &measured,
                            drawn: &drawn,
                            effects: &effects,
                            final_size: &final_size,
                        }))
                    });
                    assert_eq!(measured.get(), 0);
                    assert_eq!(drawn.get(), 0);
                });
            },
        )
        .drop_without_applying_deltas();
        assert_eq!(drawn.get(), 1);
        assert_eq!(effects.get(), 1);
        // Known stretched widths take precedence over the callback's 200px preference.
        assert!((final_size.get()[0] - (width - 20.0)).abs() < 1.0);
        sizes.push(final_size.get());
    }
    assert!(sizes[1][1] > sizes[0][1]);
}
