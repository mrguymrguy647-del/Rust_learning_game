//! A small code editor: line numbers, Rust syntax highlighting, Tab = 4 spaces,
//! Shift+Tab = dedent, auto-indent on Enter, and error-line markers.

use eframe::egui::{self, text::CCursor, text::CCursorRange, Key, Modifiers, TextBuffer};
use egui_extras::syntax_highlighting::{highlight, CodeTheme};

use crate::theme::Palette;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Marker {
    Error,
    Warning,
}

pub struct CodeEditor<'a> {
    pub code: &'a mut String,
    pub id: egui::Id,
    /// `(1-based line, marker)` pairs.
    pub markers: &'a [(u32, Marker)],
    pub min_height: f32,
}

const INDENT: &str = "    ";

fn byte_index(text: &str, char_index: usize) -> usize {
    text.char_indices().nth(char_index).map(|(b, _)| b).unwrap_or(text.len())
}

/// Start (in chars) of the line containing char index `at`.
fn line_start_chars(text: &str, at: usize) -> usize {
    let before: Vec<char> = text.chars().take(at).collect();
    before.iter().rposition(|&c| c == '\n').map(|p| p + 1).unwrap_or(0)
}

fn leading_whitespace(line: &str) -> String {
    line.chars().take_while(|c| *c == ' ' || *c == '\t').collect()
}

impl CodeEditor<'_> {
    pub fn show(self, ui: &mut egui::Ui) -> egui::Response {
        let CodeEditor { code, id, markers, min_height } = self;
        let pal = Palette::of(ui);
        let ctx = ui.ctx().clone();
        let focused = ctx.memory(|m| m.has_focus(id));

        // Key handling must happen before the TextEdit sees the events.
        if focused {
            handle_keys(&ctx, id, code);
        }

        let theme = CodeTheme::from_style(ui.style());
        let mut layouter = |ui: &egui::Ui, buf: &dyn TextBuffer, _wrap: f32| {
            let mut job = highlight(ui.ctx(), ui.style(), &theme, buf.as_str(), "rs");
            job.wrap.max_width = f32::INFINITY;
            ui.fonts_mut(|f| f.layout_job(job))
        };

        let line_count = code.lines().count().max(1) + usize::from(code.ends_with('\n'));
        let font_id = egui::TextStyle::Monospace.resolve(ui.style());
        let gutter_digits = line_count.to_string().len().max(2);
        let gutter_text: String = (1..=line_count)
            .map(|n| format!("{n:>width$}", width = gutter_digits))
            .collect::<Vec<_>>()
            .join("\n");

        let mut response = None;
        egui::Frame::new()
            .fill(pal.code_bg)
            .stroke(egui::Stroke::new(1.0, pal.border))
            .corner_radius(8)
            .show(ui, |ui| {
                egui::ScrollArea::both().auto_shrink([false, false]).min_scrolled_height(min_height).show(
                    ui,
                    |ui| {
                        ui.horizontal_top(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            // Gutter. Same font => same row height as the editor, so numbers line up.
                            ui.add_space(6.0);
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(gutter_text).font(font_id.clone()).color(pal.dim),
                                )
                                .wrap_mode(egui::TextWrapMode::Extend)
                                .selectable(false),
                            );
                            ui.add_space(8.0);
                            let out = egui::TextEdit::multiline(code)
                                .id(id)
                                .font(egui::TextStyle::Monospace)
                                .frame(egui::Frame::NONE)
                                .desired_width(f32::INFINITY)
                                .desired_rows(((min_height / font_id.size.max(1.0)) as usize).max(10))
                                .lock_focus(true)
                                .layouter(&mut layouter)
                                .show(ui);

                            // Mark problem lines.
                            for (line, marker) in markers {
                                if let Some(row) = out.galley.rows.get(line.saturating_sub(1) as usize) {
                                    let color = match marker {
                                        Marker::Error => pal.bad,
                                        Marker::Warning => pal.warn,
                                    };
                                    let rect = row.rect().translate(out.galley_pos.to_vec2());
                                    let wide = egui::Rect::from_min_max(
                                        egui::pos2(ui.min_rect().left(), rect.top()),
                                        egui::pos2(ui.max_rect().right().max(rect.right()), rect.bottom()),
                                    );
                                    ui.painter().rect_filled(wide, 0.0, color.gamma_multiply(0.16));
                                    ui.painter().rect_filled(
                                        egui::Rect::from_min_size(wide.min, egui::vec2(3.0, wide.height())),
                                        0.0,
                                        color,
                                    );
                                }
                            }
                            response = Some(out.response.response);
                        });
                    },
                );
            });
        response.unwrap_or_else(|| ui.label(""))
    }
}

fn handle_keys(ctx: &egui::Context, id: egui::Id, code: &mut String) {
    let tab = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Tab));
    let shift_tab = ctx.input_mut(|i| i.consume_key(Modifiers::SHIFT, Key::Tab));
    let enter = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));
    if !(tab || shift_tab || enter) {
        return;
    }
    let Some(mut state) = egui::TextEdit::load_state(ctx, id) else {
        return;
    };
    let Some(range) = state.cursor.char_range() else {
        return;
    };
    let (a, b) = (range.primary.index.0, range.secondary.index.0);
    let (lo, hi) = (a.min(b), a.max(b));

    if enter {
        let start = line_start_chars(code, lo);
        let line: String = code.chars().skip(start).take(lo - start).collect();
        let mut indent = leading_whitespace(&line);
        let trimmed = line.trim_end();
        if trimmed.ends_with('{') || trimmed.ends_with('(') || trimmed.ends_with('[') {
            indent.push_str(INDENT);
        }
        let insert = format!("\n{indent}");
        let (b_lo, b_hi) = (byte_index(code, lo), byte_index(code, hi));
        code.replace_range(b_lo..b_hi, &insert);
        let new_pos = lo + insert.chars().count();
        state.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(new_pos))));
    } else if tab && lo == hi {
        let start = line_start_chars(code, lo);
        let col = lo - start;
        let spaces = INDENT.len() - (col % INDENT.len());
        let b = byte_index(code, lo);
        code.insert_str(b, &" ".repeat(spaces));
        state.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(lo + spaces))));
    } else if tab || shift_tab {
        // Indent / dedent every selected line.
        let first_line_start = line_start_chars(code, lo);
        let last_line_start = line_start_chars(code, hi);
        let b_first = byte_index(code, first_line_start);
        let end_of_last = code[byte_index(code, last_line_start)..]
            .find('\n')
            .map(|n| byte_index(code, last_line_start) + n)
            .unwrap_or(code.len());
        let block = code[b_first..end_of_last].to_string();
        let mut shift_total: isize = 0;
        let new_block: Vec<String> = block
            .split('\n')
            .map(|l| {
                if tab {
                    shift_total += INDENT.len() as isize;
                    format!("{INDENT}{l}")
                } else {
                    let remove = l.chars().take(INDENT.len()).take_while(|c| *c == ' ').count();
                    shift_total -= remove as isize;
                    l.chars().skip(remove).collect()
                }
            })
            .collect();
        code.replace_range(b_first..end_of_last, &new_block.join("\n"));
        let new_hi = (hi as isize + shift_total).max(0) as usize;
        state
            .cursor
            .set_char_range(Some(CCursorRange::two(CCursor::new(first_line_start), CCursor::new(new_hi))));
    }
    state.store(ctx, id);
}
