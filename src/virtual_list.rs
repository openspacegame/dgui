//! Virtual rows with declared extents and stable scroll anchoring.

use std::{
    hash::Hash,
    sync::{Arc, Mutex},
};

use crate::{AvailableSpace, Dgui, Frame, Length, Style, Ui};

/// A scrolling viewport that constructs only intersecting rows and overscan.
///
/// Each height includes all row padding. Contents are clipped to that height.
/// Keys identify rows across insertions and sorting. Keep persistent state and
/// tasks in an ancestor scope: rows outside the viewport unmount.
/// Give the resulting frame a bounded height, or let it grow in a bounded column.
pub struct VirtualList<'a> {
    heights: Vec<f32>,
    key: Box<dyn Fn(usize) -> egui::Id + 'a>,
    row: Box<dyn FnMut(&mut Ui<'_, 'a>, usize) + 'a>,
    reveal: Option<egui::Id>,
    header: Option<(f32, Box<dyn FnOnce(&mut Ui<'_, 'a>) + 'a>)>,
}

impl<'a> VirtualList<'a> {
    /// Declare row heights in logical points. Heights must be positive and finite.
    pub fn new<K: Hash>(
        heights: impl IntoIterator<Item = f32>,
        key: impl Fn(usize) -> K + 'a,
        row: impl FnMut(&mut Ui<'_, 'a>, usize) + 'a,
    ) -> Self {
        let heights: Vec<_> = heights.into_iter().collect();
        assert!(
            heights
                .iter()
                .all(|height| height.is_finite() && *height > 0.0)
        );
        Self {
            heights,
            key: Box::new(move |index| egui::Id::new(crate::hash(key(index)))),
            row: Box::new(row),
            reveal: None,
            header: None,
        }
    }

    /// Construct a list whose rows all have the same declared height.
    pub fn uniform<K: Hash>(
        count: usize,
        height: f32,
        key: impl Fn(usize) -> K + 'a,
        row: impl FnMut(&mut Ui<'_, 'a>, usize) + 'a,
    ) -> Self {
        assert!(height.is_finite() && height > 0.0);
        Self::new(std::iter::repeat_n(height, count), key, row)
    }

    /// Reveal a row with minimal scrolling. Supply this only for a new request.
    /// An absent key has no effect.
    pub fn reveal(mut self, key: impl Hash) -> Self {
        self.reveal = Some(egui::Id::new(crate::hash(key)));
        self
    }

    /// Prepend content with a declared height inside the same scroll viewport.
    /// A table header then receives exactly the same width as its rows.
    /// The header scrolls with the list and is outside the row index space.
    pub fn header(mut self, height: f32, draw: impl FnOnce(&mut Ui<'_, 'a>) + 'a) -> Self {
        assert!(height.is_finite() && height > 0.0);
        self.header = Some((height, Box::new(draw)));
        self
    }

    /// Build a frame that owns its vertical scroll viewport.
    pub fn frame(mut self) -> Frame<'a> {
        let keys: Vec<_> = (0..self.heights.len()).map(&self.key).collect();
        let header_height = self.header.as_ref().map_or(0.0, |(height, _)| *height);
        let offsets: Vec<_> = offsets(&self.heights)
            .into_iter()
            .map(|offset| offset + header_height)
            .collect();
        Frame::canvas(
            move |_, input| {
                let width = input.known[0].unwrap_or(match input.available[0] {
                    AvailableSpace::Definite(width) => width,
                    AvailableSpace::MinContent | AvailableSpace::MaxContent => 0.0,
                });
                [width, 0.0]
            },
            move |canvas| {
                let memory_id = canvas.id.with("virtual-memory");
                let mut memory = canvas.ui.ctx().data_mut(|data| {
                    data.remove_temp::<ListMemory>(memory_id)
                        .unwrap_or_default()
                });
                let viewport_height = canvas.content_rect.height();
                let changed = memory.keys != keys || memory.offsets != offsets;
                let mut top = if changed {
                    anchored_top(&memory, &keys, &offsets)
                } else {
                    memory.top
                };
                let reveal = self
                    .reveal
                    .and_then(|key| keys.iter().position(|value| *value == key));
                if let Some(index) = reveal {
                    top = reveal_top(top, viewport_height, offsets[index], offsets[index + 1]);
                }
                top = top.clamp(0.0, (offsets.last().unwrap() - viewport_height).max(0.0));
                let mut scroll = egui::ScrollArea::vertical()
                    .id_salt(canvas.id.with("virtual-scroll"))
                    .auto_shrink([false, false]);
                if changed || reveal.is_some() {
                    scroll = scroll.vertical_scroll_offset(top);
                }
                let output = scroll.show_viewport(canvas.ui, |host, viewport| {
                    let range = visible_rows(&offsets, viewport.min.y, viewport.max.y);
                    let style = Style::column()
                        .width(Length::Percent(1.0))
                        .height(*offsets.last().unwrap());
                    memory
                        .runtime
                        .lock()
                        .unwrap()
                        .show_styled(host, style, |ui| {
                            if let Some((height, header)) = self.header.take() {
                                ui.scope("header", |ui| {
                                    ui.frame(Style::column().height(height).shrink(0.0), header);
                                });
                            }
                            ui.add(
                                Frame::new()
                                    .height(offsets[range.start] - header_height)
                                    .shrink(0.0),
                            );
                            for index in range.clone() {
                                ui.scope(keys[index], |ui| {
                                    ui.frame(
                                        Style::column().height(self.heights[index]).shrink(0.0),
                                        |ui| {
                                            (self.row)(ui, index);
                                        },
                                    );
                                });
                            }
                            ui.add(
                                Frame::new()
                                    .height(offsets.last().unwrap() - offsets[range.end])
                                    .shrink(0.0),
                            );
                        });
                });
                memory.keys = keys;
                memory.offsets = offsets;
                memory.top = output.state.offset.y;
                canvas
                    .ui
                    .ctx()
                    .data_mut(|data| data.insert_temp(memory_id, memory));
            },
        )
        .grow(1.0)
    }
}

#[derive(Clone)]
struct ListMemory {
    runtime: Arc<Mutex<Dgui>>,
    keys: Vec<egui::Id>,
    offsets: Vec<f32>,
    top: f32,
}

impl Default for ListMemory {
    fn default() -> Self {
        Self {
            runtime: Arc::new(Mutex::new(Dgui::new())),
            keys: Vec::new(),
            offsets: vec![0.0],
            top: 0.0,
        }
    }
}

fn offsets(heights: &[f32]) -> Vec<f32> {
    let mut offsets = Vec::with_capacity(heights.len() + 1);
    offsets.push(0.0);
    for height in heights {
        offsets.push(offsets.last().unwrap() + height);
    }
    assert!(offsets.last().unwrap().is_finite());
    offsets
}

fn anchored_top(memory: &ListMemory, keys: &[egui::Id], offsets: &[f32]) -> f32 {
    if memory.keys.is_empty() {
        return 0.0;
    }
    let anchor = memory
        .offsets
        .partition_point(|offset| *offset <= memory.top)
        .saturating_sub(1)
        .min(memory.keys.len() - 1);
    if let Some(index) = keys.iter().position(|key| *key == memory.keys[anchor]) {
        return offsets[index]
            + (memory.top - memory.offsets[anchor]).min(offsets[index + 1] - offsets[index]);
    }
    let positions: ahash::AHashMap<_, _> = keys
        .iter()
        .enumerate()
        .map(|(index, key)| (*key, index))
        .collect();
    for key in memory.keys[..anchor].iter().rev() {
        if let Some(index) = positions.get(key) {
            return offsets[*index];
        }
    }
    0.0
}

fn reveal_top(top: f32, height: f32, start: f32, end: f32) -> f32 {
    if start < top || end - start > height {
        start
    } else if end > top + height {
        end - height
    } else {
        top
    }
}

fn visible_rows(offsets: &[f32], top: f32, bottom: f32) -> std::ops::Range<usize> {
    let count = offsets.len() - 1;
    let first = offsets
        .partition_point(|offset| *offset <= top)
        .saturating_sub(1);
    let last = offsets.partition_point(|offset| *offset < bottom);
    first.saturating_sub(2).min(count)..last.saturating_add(2).min(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixed_rows_reveal_their_full_extent() {
        let offsets = offsets(&[20.0, 40.0, 20.0, 40.0]);
        assert_eq!(reveal_top(0.0, 50.0, offsets[2], offsets[3]), 30.0);
        assert_eq!(reveal_top(70.0, 50.0, offsets[1], offsets[2]), 20.0);
        assert_eq!(reveal_top(20.0, 50.0, offsets[1], offsets[2]), 20.0);
    }

    #[test]
    fn insertion_keeps_the_same_row_at_the_viewport_top() {
        let keys: Vec<_> = (0..3).map(egui::Id::new).collect();
        let memory = ListMemory {
            keys: keys.clone(),
            offsets: offsets(&[20.0, 40.0, 20.0]),
            top: 25.0,
            ..Default::default()
        };
        let changed = vec![egui::Id::new(9), keys[0], keys[1], keys[2]];
        assert_eq!(
            anchored_top(&memory, &changed, &offsets(&[30.0, 20.0, 40.0, 20.0])),
            55.0
        );
        assert_eq!(
            anchored_top(&memory, &[keys[0], keys[2]], &offsets(&[20.0, 20.0])),
            0.0
        );
    }

    #[test]
    fn construction_range_is_bounded_by_the_viewport() {
        let offsets = offsets(&vec![20.0; 10_000]);
        assert_eq!(visible_rows(&offsets, 100_000.0, 100_300.0), 4998..5017);
        assert_eq!(visible_rows(&[0.0], 0.0, 300.0), 0..0);
        assert_eq!(visible_rows(&[0.0, 20.0], 100_000.0, 100_300.0), 0..1);
    }
}
