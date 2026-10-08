//! Reproducible CPU benchmark: cargo run --release --example overhead > samples.csv
//! Optional environment: DGUI_BENCH_MS=150 DGUI_BENCH_ROUNDS=5 DGUI_BENCH_FILTER=calculator
//! CPU UI work only by default; DGUI_BENCH_TESSELLATE=1 includes tessellation.
use dgui::{Align, Dgui, Frame, Length, Style};
use egui::{Color32, FontId, RichText, Vec2};
use std::{cell::Cell, hint::black_box, time::Instant};

const BUTTON: Vec2 = egui::vec2(72.0, 40.0);
const GAP: f32 = 8.0;
const KEYS: [&str; 20] = [
    "C", "±", "%", "÷", "7", "8", "9", "×", "4", "5", "6", "−", "1", "2", "3", "+", "0", ".", "⌫",
    "=",
];

#[derive(Clone, Copy, PartialEq)]
enum Backend {
    Egui,
    DguiNative,
    Dgui,
}
impl Backend {
    fn name(self) -> &'static str {
        match self {
            Self::Egui => "egui",
            Self::DguiNative => "dgui_native",
            Self::Dgui => "dgui",
        }
    }
}

#[derive(Clone, Copy)]
struct Case {
    name: &'static str,
    count: usize,
    calculator: bool,
    text: bool,
    reflow: bool,
    clipped: bool,
}

fn rich(text: &str) -> RichText {
    RichText::new(text)
        .font(FontId::proportional(16.0))
        .color(Color32::from_rgb(225, 231, 240))
}

fn native_button(ui: &mut egui::Ui, label: &str, clicked: &Cell<usize>) {
    if ui
        .add_sized(
            BUTTON,
            egui::Button::new(rich(label))
                .fill(Color32::from_rgb(49, 67, 91))
                .stroke(egui::Stroke::NONE)
                .corner_radius(6),
        )
        .clicked()
    {
        clicked.set(clicked.get().wrapping_add(1));
    }
}

fn dgui_button<'a>(backend: Backend, label: &'a str, clicked: &'a Cell<usize>) -> Frame<'a> {
    let frame = if backend == Backend::DguiNative {
        Frame::egui_canvas(BUTTON.into(), move |ui| native_button(ui, label, clicked))
    } else {
        dgui::button(label).on_click(move || clicked.set(clicked.get().wrapping_add(1)))
    };
    frame.width(BUTTON.x).height(BUTTON.y).shrink(0.0)
}

struct Runner {
    ctx: egui::Context,
    gui: Dgui,
    backend: Backend,
    case: Case,
    labels: Vec<String>,
    clicked: Cell<usize>,
    frame: usize,
    tessellate: bool,
}

#[derive(Default)]
struct Timing {
    ui_us: f64,
    tess_us: f64,
    total_us: f64,
    shapes: usize,
    vertices: usize,
    primitives: usize,
}

impl Runner {
    fn new(backend: Backend, case: Case) -> Self {
        let ctx = egui::Context::default();
        ctx.options_mut(|o| o.max_passes = 1.try_into().unwrap());
        ctx.all_styles_mut(|s| {
            s.spacing.item_spacing = egui::vec2(GAP, GAP);
            s.spacing.button_padding = egui::vec2(8.0, 8.0);
            s.animation_time = 0.0;
        });
        Self {
            ctx,
            gui: Dgui::new(),
            backend,
            case,
            labels: (0..case.count)
                .map(|i| {
                    if case.calculator {
                        KEYS[i].to_owned()
                    } else if case.text {
                        format!("Row {i}: A responsive interface lays out this paragraph again as the available width changes. Each row has unique text, with enough words to wrap over several lines in a narrow window.")
                    } else {
                        format!("{i}")
                    }
                })
                .collect(),
            clicked: Cell::new(0),
            frame: 0,
            tessellate: std::env::var("DGUI_BENCH_TESSELLATE").as_deref() == Ok("1"),
        }
    }

    fn step(&mut self) -> Timing {
        self.step_audited(None)
    }

    fn step_audited(&mut self, audit: Option<&mut Vec<egui::Rect>>) -> Timing {
        let start = Instant::now();
        let case = self.case;
        // A triangular sweep changes width every frame, and repeatedly crosses wrap boundaries.
        let phase = self.frame % 60;
        let phase = phase.min(60 - phase) as f32;
        let width = if case.calculator {
            312.0
        } else if case.reflow {
            360.0 + phase * 28.0
        } else {
            1000.0
        };
        let height = if case.calculator || case.clipped {
            720.0
        } else {
            // Deliberately huge viewport: all widgets contribute paint/tessellation work.
            case.count as f32 * 160.0
        };
        let display = if case.calculator && case.reflow {
            format!("{}", self.frame % 100_000)
        } else {
            "12345.6789".to_owned()
        };
        let backend = self.backend;
        let labels = &self.labels;
        let clicked = &self.clicked;
        let output = self.ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width, height),
                )),
                time: Some(self.frame as f64 / 60.0),
                ..Default::default()
            },
            |host| {
                if backend == Backend::Egui {
                    if case.calculator {
                        host.add_sized([width, 40.0], egui::Label::new(rich(&display)));
                    }
                    if case.text {
                        for label in labels {
                            host.add(egui::Label::new(rich(label)).wrap());
                        }
                    } else {
                        host.horizontal_wrapped(|ui| {
                            for label in labels {
                                native_button(ui, label, clicked);
                            }
                        });
                    }
                } else {
                    self.gui.show_styled(
                        host,
                        Style::column().width(Length::Percent(1.0)).gap(GAP),
                        |ui| {
                            if case.calculator {
                                ui.add(Frame::text(&display).height(40.0).shrink(0.0));
                            }
                            if case.text {
                                for label in labels {
                                    ui.add(Frame::text(label).shrink(0.0));
                                }
                            } else {
                                ui.frame(
                                    Style::row()
                                        .wrap(true)
                                        .gap(GAP)
                                        .align(Align::Start)
                                        .width(Length::Percent(1.0))
                                        .shrink(0.0),
                                    |ui| {
                                        for label in labels {
                                            ui.add(dgui_button(backend, label, clicked));
                                        }
                                    },
                                );
                            }
                        },
                    );
                }
            },
        );
        let after_ui = Instant::now();
        if let Some(audit) = audit {
            for shape in &output.shapes {
                match &shape.shape {
                    egui::epaint::Shape::Rect(rect)
                        if !case.text && shape.clip_rect.intersects(rect.rect) =>
                    {
                        audit.push(rect.rect);
                    }
                    egui::epaint::Shape::Text(text) if case.text => {
                        audit.push(text.galley.rect.translate(text.pos.to_vec2()));
                    }
                    _ => {}
                }
            }
        }
        let shapes = output.shapes.len();
        let mut output = output;
        let primitives = if self.tessellate {
            self.ctx
                .tessellate(std::mem::take(&mut output.shapes), output.pixels_per_point)
        } else {
            Vec::new()
        };
        let after_tess = Instant::now();
        let vertices = primitives
            .iter()
            .map(|p| match &p.primitive {
                egui::epaint::Primitive::Mesh(mesh) => mesh.vertices.len(),
                _ => 0,
            })
            .sum();
        black_box(&primitives);
        black_box(clicked.get());
        let primitive_count = primitives.len();
        drop(primitives);
        output.drop_without_applying_deltas();
        self.frame += 1;
        Timing {
            ui_us: (after_ui - start).as_secs_f64() * 1e6,
            tess_us: (after_tess - after_ui).as_secs_f64() * 1e6,
            total_us: start.elapsed().as_secs_f64() * 1e6,
            shapes,
            vertices,
            primitives: primitive_count,
        }
    }
}

fn main() {
    let milliseconds: u64 = std::env::var("DGUI_BENCH_MS")
        .unwrap_or_else(|_| "150".into())
        .parse()
        .unwrap();
    let rounds: usize = std::env::var("DGUI_BENCH_ROUNDS")
        .unwrap_or_else(|_| "5".into())
        .parse()
        .unwrap();
    assert!(milliseconds > 0 && rounds > 0);
    let filter = std::env::var("DGUI_BENCH_FILTER").unwrap_or_default();
    let cases = [
        ("calculator_idle", 20, true, false, false, false),
        ("calculator_changing", 20, true, false, true, false),
        ("buttons_1000", 1000, false, false, false, false),
        ("buttons_1000_reflow", 1000, false, false, true, false),
        ("buttons_10000", 10000, false, false, false, false),
        ("buttons_10000_reflow", 10000, false, false, true, false),
        (
            "buttons_10000_clipped_reflow",
            10000,
            false,
            false,
            true,
            true,
        ),
        ("paragraphs_1000", 1000, false, true, false, false),
        ("paragraphs_1000_reflow", 1000, false, true, true, false),
    ];
    println!("case,backend,round,frames,ui_us,tess_us,total_us,shapes,vertices,primitives");
    for (name, count, calculator, text, reflow, clipped) in cases {
        if !name.contains(&filter) {
            continue;
        }
        let case = Case {
            name,
            count,
            calculator,
            text,
            reflow,
            clipped,
        };
        let backends = if text {
            vec![Backend::Egui, Backend::Dgui]
        } else {
            vec![Backend::Egui, Backend::DguiNative, Backend::Dgui]
        };
        let mut runners: Vec<_> = backends.into_iter().map(|b| Runner::new(b, case)).collect();
        // Check paint geometry at narrow, intermediate and wide widths before timing.
        // Only button rectangles are compared in the calculator (display alignment differs).
        for frame in [0, 15, 30] {
            let mut reference = Vec::new();
            for (index, runner) in runners.iter_mut().enumerate() {
                runner.frame = frame;
                let mut rects = Vec::new();
                runner.step_audited(Some(&mut rects));
                if !case.clipped {
                    assert_eq!(rects.len(), case.count, "{} missing widgets", case.name);
                }
                if index == 0 {
                    reference = rects;
                } else {
                    assert_eq!(reference.len(), rects.len(), "{} paint count", case.name);
                    for (a, b) in reference.iter().zip(&rects) {
                        assert!(
                            (a.min - b.min).length() < 1.0 && (a.max - b.max).length() < 1.0,
                            "{} {} geometry differs: {a:?} vs {b:?}",
                            case.name,
                            runner.backend.name()
                        );
                    }
                }
            }
        }
        for runner in &mut runners {
            runner.frame = 0;
            for _ in 0..32 {
                black_box(runner.step());
            }
        }
        // Calibrate once using the slowest backend, then run identical frame sequences.
        let mut slowest: f64 = 0.0;
        for runner in &mut runners {
            let start = Instant::now();
            for _ in 0..8 {
                black_box(runner.step());
            }
            slowest = slowest.max(start.elapsed().as_secs_f64() / 8.0);
        }
        // Whole width cycles keep the reflow distribution identical in every batch.
        let frames = ((milliseconds as f64 / 1000.0 / slowest).ceil() as usize).div_ceil(60) * 60;
        eprintln!("{}: {frames} frames/batch, {rounds} rounds", case.name);
        for round in 0..rounds {
            // Rotate execution order to reduce systematic clock/thermal bias.
            for offset in 0..runners.len() {
                let index = (round + offset) % runners.len();
                let runner = &mut runners[index];
                let mut sum = Timing::default();
                for _ in 0..frames {
                    let t = runner.step();
                    sum.ui_us += t.ui_us;
                    sum.tess_us += t.tess_us;
                    sum.total_us += t.total_us;
                    sum.shapes += t.shapes;
                    sum.vertices += t.vertices;
                    sum.primitives += t.primitives;
                }
                let n = frames as f64;
                println!(
                    "{},{},{},{},{:.3},{:.3},{:.3},{:.1},{:.1},{:.1}",
                    case.name,
                    runner.backend.name(),
                    round,
                    frames,
                    sum.ui_us / n,
                    sum.tess_us / n,
                    sum.total_us / n,
                    sum.shapes as f64 / n,
                    sum.vertices as f64 / n,
                    sum.primitives as f64 / n
                );
            }
        }
    }
}
