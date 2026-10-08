//! Fixed-height virtualization within the ordinary frame abstraction.

use std::{
    hash::Hash,
    sync::{Arc, Mutex},
};

use crate::{AvailableSpace, Dgui, Frame, Length, Style, Ui};

impl<'frame> Frame<'frame> {
    /// A vertically scrolling list of `row_count` rows of equal height that
    /// builds only the rows in view, so it stays fast with millions of rows.
    ///
    /// - `row_height` is each row's full height in points, including any
    ///   padding or spacing; row contents are clipped to it.
    /// - `key` gives each row index a stable key. Each row is built in its own
    ///   [`scope`](Ui::scope) with that key.
    /// - `row` builds the row at an index.
    ///
    /// The list grows to fill its parent along the main axis, so give the
    /// parent (or the list) a bounded height. Rows scrolled out of view
    /// unmount, dropping their state, so keep anything that must survive, such
    /// as selection or drafts, in state outside the list.
    ///
    /// The rows are built in a separate runtime, so they can't
    /// [declare](Ui::state) state in the list's scope, but can use [`State`](crate::State)
    /// handles captured from outside.
    ///
    /// # Panics
    ///
    /// If `row_height` isn't positive and finite.
    ///
    /// ```
    /// # fn build(ui: &mut dgui::Ui<'_, '_>) {
    /// use dgui::Frame;
    ///
    /// ui.add(Frame::virtual_list(
    ///     1_000_000,
    ///     24.0,
    ///     |index| index,
    ///     |ui, index| ui.add(Frame::text(format!("Row {index}"))),
    /// ));
    /// # }
    /// ```
    pub fn virtual_list<K: Hash>(
        row_count: usize,
        row_height: f32,
        key: impl Fn(usize) -> K + 'frame,
        mut row: impl FnMut(&mut Ui<'_, 'frame>, usize) + 'frame,
    ) -> Self {
        assert!(row_height.is_finite() && row_height > 0.0);
        Self::canvas(
            move |_, input| {
                // Use normal flex stretching for the viewport. A percentage
                // width creates a circular intrinsic constraint in auto-width
                // parents and can resolve to zero before rows are constructed.
                let width = input.known[0].unwrap_or(match input.available[0] {
                    AvailableSpace::Definite(width) => width,
                    AvailableSpace::MinContent | AvailableSpace::MaxContent => 0.0,
                });
                [width, 0.0]
            },
            move |canvas| {
                let runtime = canvas.ui.ctx().data_mut(|data| {
                    data.get_temp_mut_or_insert_with(canvas.id.with("virtual-runtime"), || {
                        Arc::new(Mutex::new(Dgui::new()))
                    })
                    .clone()
                });
                egui::ScrollArea::vertical()
                    .id_salt(canvas.id.with("virtual-scroll"))
                    .auto_shrink([false, false])
                    .show_viewport(canvas.ui, |host, viewport| {
                        let range =
                            visible_rows(viewport.min.y, viewport.max.y, row_height, row_count);
                        let style = Style::column()
                            .width(Length::Percent(1.0))
                            .height(row_count as f32 * row_height);
                        runtime.lock().unwrap().show_styled(host, style, |ui| {
                            ui.add(
                                Frame::new()
                                    .height(range.start as f32 * row_height)
                                    .shrink(0.0),
                            );
                            for index in range.clone() {
                                ui.scope(key(index), |ui| {
                                    ui.frame(
                                        Style::column().height(row_height).shrink(0.0),
                                        |ui| row(ui, index),
                                    );
                                });
                            }
                            ui.add(
                                Frame::new()
                                    .height((row_count - range.end) as f32 * row_height)
                                    .shrink(0.0),
                            );
                        });
                    });
            },
        )
        .grow(1.0)
    }
}

fn visible_rows(top: f32, bottom: f32, height: f32, count: usize) -> std::ops::Range<usize> {
    let first = (top.max(0.0) / height).floor() as usize;
    let last = (bottom.max(0.0) / height).ceil() as usize;
    first.saturating_sub(2).min(count)..last.saturating_add(2).min(count)
}

#[cfg(test)]
mod tests {
    use super::visible_rows;

    #[test]
    fn rows_are_visible_in_offset_auto_width_root() {
        use crate::{Dgui, Frame, Length, Style};
        let context = egui::Context::default();
        context.options_mut(|options| options.max_passes = 1.try_into().unwrap());
        let mut gui = Dgui::new();
        let rows = std::cell::RefCell::new(Vec::new());
        context
            .run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1600.0, 900.0),
                    )),
                    ..Default::default()
                },
                |host| {
                    let mut panel =
                        host.new_child(egui::UiBuilder::new().max_rect(egui::Rect::from_min_size(
                            egui::pos2(1165.0, 355.0),
                            egui::vec2(400.0, 350.0),
                        )));
                    gui.show_styled(
                        &mut panel,
                        Style::column().height(Length::Percent(1.0)).gap(6.0),
                        |ui| {
                            ui.add(Frame::new().width(346.0).height(164.0).shrink(0.0));
                            ui.add(Frame::virtual_list(
                                9,
                                32.0,
                                |index| index,
                                |ui, _| {
                                    ui.add(Frame::canvas(
                                        |_, input| [input.width(), 32.0],
                                        |canvas| {
                                            rows.borrow_mut()
                                                .push((canvas.content_rect, canvas.ui.clip_rect()));
                                        },
                                    ));
                                },
                            ));
                            ui.add(Frame::new().width(346.0).height(20.0).shrink(0.0));
                        },
                    );
                },
            )
            .drop_without_applying_deltas();
        assert!(!rows.borrow().is_empty());
        let (rect, clip) = rows.borrow()[0];
        assert!(
            rect.width() > 300.0,
            "row width: {rect:?}, outer layout: {:?}",
            gui.last_layout
        );
        assert!(
            rect.intersect(clip).is_positive(),
            "row {rect:?} outside clip {clip:?}"
        );
    }

    #[test]
    fn visible_work_is_bounded_and_clamped_after_filtering() {
        assert_eq!(
            visible_rows(100_000.0, 100_300.0, 20.0, 1_000_000),
            4998..5017
        );
        assert_eq!(visible_rows(100_000.0, 100_300.0, 20.0, 3), 3..3);
        assert_eq!(visible_rows(0.0, 300.0, 20.0, 0), 0..0);
    }
}
