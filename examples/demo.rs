use dgui::{Align, Color, Dgui, Frame, Length, State, Style, Ui, button, text_input};

#[derive(Default)]
struct Demo {
    gui: Dgui,
    frames: u64,
}

fn card<'a>(ui: &mut Ui<'_, 'a>, key: u32) -> Frame<'a> {
    let name = ui.state("name", || format!("Pilot {key}"));
    let score = ui.state("score", || 0);
    Frame::new()
        .gap(12.0)
        .padding(18.0)
        .width(250.0)
        .max_width(Length::Percent(1.0))
        .grow(1.0)
        .shrink(0.0)
        .background(Color::rgb(28, 38, 55))
        .corner_radius(12)
        .children(move |ui| {
            ui.add(Frame::text(format!("CREW MEMBER {key}")));
            ui.add(text_input(name));
            ui.add(Frame::text(format!(
                "{} has {} points",
                name.get(),
                score.get()
            )));
            ui.add(button("Award point").on_click(move || score.update(|n| *n += 1)));
        })
}

fn counter<'a>(ui: &mut Ui<'_, 'a>) -> Frame<'a> {
    let count = ui.state("count", || 0);
    Frame::new()
        .direction(dgui::Direction::Row)
        .gap(12.0)
        .align(Align::Center)
        .children(move |ui| {
            ui.add(button("−").on_click(move || count.update(|n| *n -= 1)));
            ui.add(Frame::text(format!("Count: {}", count.get())).width(100.0));
            ui.add(button("+").on_click(move || count.update(|n| *n += 1)));
        })
}

impl eframe::App for Demo {
    fn ui(&mut self, host: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.frames += 1;
        let frames = self.frames;
        let time = host.input(|input| input.time);
        host.ctx().request_repaint();
        self.gui.show(host, |ui| {
            let reverse: State<bool> = ui.state("reverse", || false);
            let second: State<bool> = ui.state("second", || true);
            let canvas_clicks = ui.state("canvas_clicks", || 0);
            ui.add(Frame::new().gap(20.0).padding(24.0).width(Length::Percent(1.0))
                .min_height(Length::Percent(1.0)).shrink(0.0).background(Color::rgb(12, 18, 29))
                .children(move |ui| {
                    ui.add(Frame::text("dgui / a fresh frame tree, every render cycle"));
                    ui.add(Frame::text(concat!(
                        "Everything here is a frame. Resize the window to wrap the crew cards. ",
                        "Edit names, award points, and reverse the cards: their keyed state follows them. ",
                        "Removing a card releases its state; recreating it starts fresh."
                    )));
                    ui.scope("counter", |ui| { let frame = counter(ui); ui.add(frame); });
                    ui.frame(Style::row().wrap(true).gap(12.0), |ui| {
                        ui.add(button("Reverse cards").on_click(move || reverse.update(|v| *v = !*v)));
                        ui.add(button(if second.get() { "Remove card 2" } else { "Recreate card 2" })
                            .on_click(move || second.update(|v| *v = !*v)));
                    });
                    ui.frame(Style::row().wrap(true).gap(16.0).shrink(0.0), |ui| {
                        let keys = if reverse.get() { [2, 1] } else { [1, 2] };
                        for key in keys {
                            if key == 1 || second.get() {
                                ui.scope(key, |ui| { let frame = card(ui, key); ui.add(frame); });
                            }
                        }
                    });
                    ui.add(Frame::text(format!("Canvas / clickable frame ({} clicks)", canvas_clicks.get())));
                    ui.add(Frame::egui_canvas([200.0, 65.0], move |ui| {
                        let rect = ui.max_rect();
                        let x = rect.left() + 16.0 + ((time as f32).sin() + 1.0) * 0.5 * (rect.width() - 32.0).max(0.0);
                        ui.painter().circle_filled(egui::pos2(x, rect.center().y), 10.0, egui::Color32::from_rgb(97, 213, 181));
                    }).height(85.0).padding(10.0).background(Color::rgb(23, 34, 49))
                        .hover_background(Color::rgb(33, 47, 66)).border(1.0, Color::rgb(50, 70, 90)).corner_radius(8)
                        .on_click(move || canvas_clicks.update(|n| *n += 1)));
                    ui.add(Frame::text(format!("Render cycle {frames} · state writes appear in the next tree")));
                }));
        });
    }
}

fn main() -> eframe::Result {
    eframe::run_native(
        "dgui — frame tree prototype",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size([900.0, 720.0]),
            ..Default::default()
        },
        Box::new(|_cc| Ok(Box::new(Demo::default()))),
    )
}
