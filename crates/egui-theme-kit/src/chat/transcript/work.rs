//! A folded run of work items: one summary line that opens to a line per call.

use egui::{Align, Layout, Rect, Sense, Stroke, TextFormat, Ui, Vec2, pos2, text::LayoutJob, vec2};

use super::DetailLevel;
use super::entry::{WorkItem, WorkKind, WorkStatus};
use super::group::{WorkStats, work_summary};
use super::markdown::{self, wrap_width};
use super::payload::{self, Part, mono_text};
use super::rich::{one_row, plain};
use super::row::RowCx;
use super::shell;
use crate::chat::types::{format_duration, item_id};
use crate::controls::{Icon, icons};

/// Whether a group opens without a user choice.
pub(crate) fn auto_open(stats: &WorkStats, detail: DetailLevel) -> bool {
    match detail {
        DetailLevel::Simple => stats.failed > 0,
        DetailLevel::Detailed => stats.failed > 0 || stats.running,
    }
}

/// Draws a work group keyed `key`, open as `chosen` or by [`auto_open`]; returns a new choice when the header was tapped.
pub(crate) fn group(
    ui: &mut Ui,
    rc: &mut RowCx<'_>,
    key: &str,
    items: &[WorkItem<'_>],
    chosen: Option<bool>,
) -> Option<bool> {
    let stats = WorkStats::of(items, rc.options.now);
    let lines = rc.options.show_thinking || items.iter().any(|i| i.kind != WorkKind::Reasoning);
    let open = lines && chosen.unwrap_or_else(|| auto_open(&stats, rc.options.detail));
    let mut choice = None;
    if header(ui, rc, key, &stats, open, lines) {
        choice = Some(!open);
    }
    if open {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let indent = rc.m.icon_sm + rc.m.gap.x / 2.0;
            let (gutter, _) = ui.allocate_exact_size(vec2(indent, 0.0), Sense::hover());
            let column = ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for item in items {
                    if item.kind == WorkKind::Reasoning && !rc.options.show_thinking {
                        continue;
                    }
                    call(ui, rc, item);
                }
            });
            let y = column.response.rect.y_range();
            ui.painter()
                .vline(gutter.center().x, y, Stroke::new(1.0, rc.p.separator));
        });
    }
    choice
}

/// The group summary line; returns whether it was tapped.
fn header(
    ui: &mut Ui,
    rc: &RowCx<'_>,
    key: &str,
    stats: &WorkStats,
    open: bool,
    toggles: bool,
) -> bool {
    let h = rc.m.row_h;
    let (rect, _) = ui.allocate_exact_size(vec2(wrap_width(ui), h), Sense::hover());
    let sense = if toggles {
        Sense::click()
    } else {
        Sense::hover()
    };
    let response = ui.interact(rect, item_id(rc.id, key, "group"), sense);
    if toggles && response.hovered() {
        ui.painter().rect_filled(rect, rc.p.radius_chip, rc.t.hover);
    }
    let size = rc.m.icon_sm;
    let mut x = rect.left();
    let slot =
        |x: f32| Rect::from_min_size(pos2(x, rect.center().y - size / 2.0), Vec2::splat(size));
    let chevron = if open {
        rc.icons.chevron_down
    } else {
        rc.icons.chevron_right
    };
    if toggles {
        chevron.paint(ui, slot(x), size, rc.p.text_faint);
    }
    x += size + rc.m.gap.x / 2.0;
    let kind_icon = if stats.thinking_only {
        rc.icons.reasoning
    } else {
        rc.icons.tool
    };
    if stats.running {
        super::row::spinner_at(ui, slot(x), rc.p.running);
    } else {
        kind_icon.paint(ui, slot(x), size, rc.p.text_dim);
    }
    x += size + rc.m.gap.x / 2.0;
    let mut job = LayoutJob::default();
    let summary = work_summary(stats);
    for (n, part) in summary.split(" \u{b7} ").enumerate() {
        if n > 0 {
            job.append(
                " \u{b7} ",
                0.0,
                TextFormat::simple(rc.f.small.clone(), rc.p.text_faint),
            );
        }
        let color = if part.ends_with("failed") {
            rc.p.error
        } else {
            rc.p.text_dim
        };
        job.append(part, 0.0, TextFormat::simple(rc.f.small.clone(), color));
    }
    job.wrap.max_width = (rect.right() - x).max(1.0);
    job.wrap.max_rows = 1;
    job.wrap.overflow_character = Some('\u{2026}');
    let galley = ui.ctx().fonts_mut(|f| f.layout_job(job));
    let y = (rect.center().y - galley.size().y / 2.0).round();
    ui.painter().galley(pos2(x, y), galley, rc.p.text_dim);
    toggles && rc.tapped(&response)
}

/// Icon of a work kind.
fn kind_icon(rc: &RowCx<'_>, kind: WorkKind) -> Icon {
    match kind {
        WorkKind::Reasoning => rc.icons.reasoning,
        WorkKind::Tool | WorkKind::Other => rc.icons.tool,
        WorkKind::Command => rc.icons.command,
        WorkKind::FileChange => rc.icons.file,
    }
}

/// Whether an item has anything to show when opened.
fn has_detail(item: &WorkItem<'_>) -> bool {
    item.arguments.is_some()
        || item.result.is_some()
        || item.body.as_deref().is_some_and(|b| !b.trim().is_empty())
        || !item.files.is_empty()
        || (item.kind == WorkKind::Command && !item.summary.is_empty())
}

/// One call line that opens in place to its arguments, result, output or files.
fn call(ui: &mut Ui, rc: &mut RowCx<'_>, item: &WorkItem<'_>) {
    let open_id = item_id(rc.id, &item.key, "call_open");
    let mut open = ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false);
    let expandable = has_detail(item);
    let h = rc.m.row_h;
    let sense = if expandable {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, _) = ui.allocate_exact_size(vec2(wrap_width(ui), h), Sense::hover());
    let response = ui.interact(rect, item_id(rc.id, &item.key, "call"), sense);
    if expandable && response.hovered() {
        ui.painter().rect_filled(rect, rc.p.radius_chip, rc.t.hover);
    }
    let size = rc.m.icon_sm;
    let slot =
        |x: f32| Rect::from_min_size(pos2(x, rect.center().y - size / 2.0), Vec2::splat(size));
    let gap = rc.m.gap.x / 2.0;
    let mut x = rect.left() + gap;
    match item.status {
        WorkStatus::Running => super::row::spinner_at(ui, slot(x), rc.p.running),
        WorkStatus::Failed => icons::ERROR.paint(ui, slot(x), size, rc.p.error),
        WorkStatus::Declined => icons::MINUS_CIRCLE.paint(ui, slot(x), size, rc.p.warn),
        WorkStatus::Pending => icons::CLOCK.paint(ui, slot(x), size, rc.p.text_faint),
        WorkStatus::Ok => kind_icon(rc, item.kind).paint(ui, slot(x), size, rc.p.text_faint),
    }
    x += size + gap;
    let right = rect.right() - gap;
    let duration = item
        .duration_ms
        .or_else(|| match (item.status, item.started, rc.options.now) {
            (WorkStatus::Running, Some(start), Some(now)) => Some(now.since(start)),
            _ => None,
        })
        .map(format_duration);
    let mut text_right = right;
    if let Some(d) = &duration {
        let g = one_row(ui, d, &rc.f.small, rc.p.text_faint, right - x);
        let pos = pos2(
            right - g.size().x,
            (rect.center().y - g.size().y / 2.0).round(),
        );
        text_right = pos.x - gap;
        ui.painter().galley(pos, g, rc.p.text_faint);
    }
    let name = match (item.kind, item.name.is_empty()) {
        (WorkKind::Reasoning, true) if item.status == WorkStatus::Running => "Thinking",
        (WorkKind::Reasoning, true) => "Thought",
        _ => item.name.as_ref(),
    };
    let name_color = if item.status == WorkStatus::Failed {
        rc.p.error
    } else {
        rc.p.text
    };
    let mut job = LayoutJob::default();
    job.append(
        name,
        0.0,
        TextFormat::simple(rc.f.small.clone(), name_color),
    );
    let summary = if item.kind == WorkKind::Reasoning && item.summary.is_empty() {
        item.body
            .as_deref()
            .and_then(|b| b.lines().find(|l| !l.trim().is_empty()))
            .unwrap_or("")
    } else {
        item.summary.as_ref()
    };
    if !summary.is_empty() {
        let first = summary.lines().next().unwrap_or("");
        if item.kind == WorkKind::Command {
            job.append(
                "  ",
                0.0,
                TextFormat::simple(rc.f.small.clone(), rc.p.text_dim),
            );
            let mono = egui::FontId::new(rc.f.small.size, rc.f.mono.family.clone());
            shell::append(&mut job, first, &mono, &rc.p.shell);
        } else {
            job.append(
                &format!("  {first}"),
                0.0,
                TextFormat::simple(rc.f.small.clone(), rc.p.text_dim),
            );
        }
    }
    let text = super::rich::renderable(ui.ctx(), &rc.f.small, &job.text).into_owned();
    if text != job.text {
        job = LayoutJob::simple_singleline(text, rc.f.small.clone(), rc.p.text_dim);
    }
    job.wrap.max_width = (text_right - x).max(1.0);
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    job.wrap.overflow_character = Some('\u{2026}');
    let galley = ui.ctx().fonts_mut(|f| f.layout_job(job));
    let pos = pos2(x, (rect.center().y - galley.size().y / 2.0).round());
    ui.painter().galley(pos, galley, rc.p.text_dim);
    if expandable && rc.tapped(&response) {
        open = !open;
        ui.data_mut(|d| d.insert_temp(open_id, open));
    }
    if open && expandable {
        let indent = size + gap * 2.0;
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.add_space(indent);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = rc.m.gap.y / 2.0;
                detail(ui, rc, item);
                ui.add_space(rc.m.gap.y / 2.0);
            });
        });
    }
}

/// The opened body of a call.
fn detail(ui: &mut Ui, rc: &mut RowCx<'_>, item: &WorkItem<'_>) {
    let id = item_id(rc.id, &item.key, "detail");
    match item.kind {
        WorkKind::Reasoning => {
            if let Some(body) = &item.body {
                let ink = rc.p.text_dim;
                markdown::show(ui, rc, body, ink, id, false);
            }
            return;
        }
        WorkKind::Command => {
            if !item.summary.is_empty() {
                markdown::code_block(ui, rc, "shell", &item.summary, id.with("command"));
            }
        }
        WorkKind::FileChange => {
            for path in item.files.iter() {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = rc.m.gap.x / 2.0;
                    rc.icons.file.show_tinted(ui, rc.m.icon_sm, rc.p.text_dim);
                    let font = rc.f.mono.clone();
                    plain(ui, rc, path, &font, rc.p.text);
                });
            }
        }
        WorkKind::Tool | WorkKind::Other => {}
    }
    if let Some(args) = &item.arguments {
        caption(ui, rc, "Arguments");
        payload::show(ui, rc, args, id.with("arguments"), Part::Arguments);
    }
    if let Some(result) = &item.result {
        caption(ui, rc, "Result");
        payload::show(ui, rc, result, id.with("result"), Part::Result);
    }
    if let Some(body) = item.body.as_deref().filter(|b| !b.trim().is_empty()) {
        match item.kind {
            WorkKind::FileChange => diff(ui, rc, body),
            WorkKind::Command => {
                caption(ui, rc, "Output");
                let ink = rc.p.text;
                mono_text(ui, rc, body, ink, id.with("output"));
            }
            _ => {
                let ink = rc.p.text;
                markdown::show(ui, rc, body, ink, id.with("body"), false);
            }
        }
    }
}

fn caption(ui: &mut Ui, rc: &RowCx<'_>, text: &str) {
    let font = rc.f.small.clone();
    plain(ui, rc, text, &font, rc.p.text_faint);
}

/// A unified diff with added lines in the success colour and removed ones in the error colour.
fn diff(ui: &mut Ui, rc: &mut RowCx<'_>, body: &str) {
    let (shown, left) = markdown::capped(body, payload::MONO_MAX_CHARS);
    let mut job = LayoutJob::default();
    for line in shown.split_inclusive('\n') {
        let color = match line.as_bytes().first() {
            Some(b'+') if !line.starts_with("+++") => rc.p.success,
            Some(b'-') if !line.starts_with("---") => rc.p.error,
            Some(b'@') => rc.p.info,
            _ => rc.p.text_dim,
        };
        job.append(line, 0.0, TextFormat::simple(rc.f.mono.clone(), color));
    }
    job.wrap.max_width = wrap_width(ui);
    job.wrap.break_anywhere = true;
    ui.with_layout(Layout::top_down(Align::Min), |ui| {
        let galley = ui.ctx().fonts_mut(|f| f.layout_job(job));
        ui.add(egui::Label::new(galley).selectable(rc.selectable));
        if left > 0 {
            markdown::more_note(ui, rc, left);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_open_on_failure_and_in_detail_while_running() {
        let running = WorkStats {
            running: true,
            ..WorkStats::default()
        };
        let failed = WorkStats {
            failed: 1,
            ..WorkStats::default()
        };
        assert!(!auto_open(&running, DetailLevel::Simple));
        assert!(auto_open(&running, DetailLevel::Detailed));
        assert!(auto_open(&failed, DetailLevel::Simple));
        assert!(!auto_open(&WorkStats::default(), DetailLevel::Detailed));
    }
}
