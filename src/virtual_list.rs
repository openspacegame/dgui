//! Fixed-height virtualization within the ordinary frame abstraction.

use std::{
    hash::Hash,
    sync::{Arc, Mutex},
};

use crate::{Dgui, Frame, Length, Style, Ui};

impl<'frame> Frame<'frame> {
    /// A vertical scroll viewport that builds only visible, keyed rows.
    ///
    /// Give the frame a height or let it grow in a bounded parent. Row height
    /// includes all padding and spacing; row contents are clipped to that height.
    /// Keep selection and editable drafts in application state, because rows
    /// leaving the overscan range unmount their local component state.
    pub fn virtual_list<K: Hash>(
        row_count: usize,
        row_height: f32,
        key: impl Fn(usize) -> K + 'frame,
        mut row: impl FnMut(&mut Ui<'_, 'frame>, usize) + 'frame,
    ) -> Self {
        assert!(row_height.is_finite() && row_height > 0.0);
        Self::canvas(
            move |_, input| [input.known[0].unwrap_or(0.0), 0.0],
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
        .width(Length::Percent(1.0))
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
    fn visible_work_is_bounded_and_clamped_after_filtering() {
        assert_eq!(
            visible_rows(100_000.0, 100_300.0, 20.0, 1_000_000),
            4998..5017
        );
        assert_eq!(visible_rows(100_000.0, 100_300.0, 20.0, 3), 3..3);
        assert_eq!(visible_rows(0.0, 300.0, 20.0, 0), 0..0);
    }
}
