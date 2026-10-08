//! The sign-in dialog from the README.
use dgui::{Align, Color, Direction, Frame, Justify, Length, Ui, button, text_input};

fn sign_in<'a>(ui: &mut Ui<'_, 'a>) -> Frame<'a> {
    let name = ui.state("name", || String::from("Ada"));

    // Fill the window and center the dialog on both axes.
    Frame::new()
        .width(Length::Percent(1.0))
        .height(Length::Percent(1.0))
        .align(Align::Center)
        .justify(Justify::Center)
        .children(move |ui| {
            ui.add(
                Frame::new()
                    .width(360.0)
                    .max_width(Length::Percent(0.9))
                    .padding(24.0)
                    .gap(16.0)
                    .background(Color::rgb(28, 38, 55))
                    .corner_radius(12)
                    .children(move |ui| {
                        // Title on the left, close button pinned to the right.
                        ui.add(
                            Frame::new()
                                .direction(Direction::Row)
                                .justify(Justify::SpaceBetween)
                                .align(Align::Center)
                                .children(|ui| {
                                    ui.add(Frame::text("Sign in"));
                                    ui.add(button("×"));
                                }),
                        );
                        ui.add(text_input(name));
                        // Actions hug the right edge, whatever their labels say.
                        ui.add(
                            Frame::new()
                                .direction(Direction::Row)
                                .justify(Justify::End)
                                .gap(8.0)
                                .children(move |ui| {
                                    ui.add(button("Cancel"));
                                    ui.add(
                                        button(format!("Continue as {}", name.get()))
                                            .on_click(move || println!("hello, {}", name.get())),
                                    );
                                }),
                        );
                    }),
            );
        })
}

struct App {
    gui: dgui::Dgui,
}

impl eframe::App for App {
    fn ui(&mut self, host: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.gui.show(host, |ui| {
            ui.scope("sign_in", |ui| {
                let frame = sign_in(ui);
                ui.add(frame);
            });
        });
    }
}

fn main() -> eframe::Result {
    eframe::run_native(
        "dgui — sign in",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size([640.0, 400.0]),
            ..Default::default()
        },
        Box::new(|_cc| {
            Ok(Box::new(App {
                gui: dgui::Dgui::new(),
            }))
        }),
    )
}
