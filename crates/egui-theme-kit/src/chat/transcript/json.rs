//! Tool payloads as `egui_json_tree` trees, sized and coloured for chat.

use std::sync::Arc;

use egui::{
    Align, Id, Label, Layout, Rect, Sense, TextFormat, TextStyle, Ui, UiBuilder, pos2,
    style::ScrollAnimation, text::LayoutJob, vec2,
};
use egui_json_tree::{
    DefaultExpand, JsonTree, JsonTreeStyle, JsonTreeVisuals,
    delimiters::ExpandableDelimiter,
    pointer::JsonPointerSegment,
    render::{DefaultRender, RenderContext},
    value::BaseValueType,
};
use serde_json::Value;

use super::shell::is_command_key;
use crate::chat::icons::ChatIcons;
use crate::chat::palette::{ChatPalette, ShellColors};
use crate::controls::{IconBox, IconButton};

/// How deep a payload opens by default.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PayloadRole {
    /// Opens two levels.
    Arguments,
    /// Opens one level.
    Result,
    Depth(u8),
}

impl PayloadRole {
    pub fn level(self) -> u8 {
        match self {
            Self::Arguments => 2,
            Self::Result => 1,
            Self::Depth(d) => d,
        }
    }
}

/// Size limits that keep large payloads cheap.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TreeBudget {
    /// Above this many nodes the tree opens at the top level only.
    pub lazy_nodes: usize,
    /// Above this many content bytes the tree opens at the top level only.
    pub lazy_bytes: usize,
    /// Above this many nodes Expand all is off and a text view is offered.
    pub max_expand_nodes: usize,
}

impl Default for TreeBudget {
    fn default() -> Self {
        Self {
            lazy_nodes: 2_000,
            lazy_bytes: 256 * 1024,
            max_expand_nodes: 20_000,
        }
    }
}

/// Nodes and approximate content bytes of `value`, counting stopped once nodes exceed `limit`.
pub fn count_nodes(value: &Value, limit: usize) -> (usize, usize) {
    let (mut nodes, mut bytes) = (0usize, 0usize);
    let mut stack = vec![value];
    while let Some(v) = stack.pop() {
        nodes += 1;
        if nodes > limit {
            break;
        }
        match v {
            Value::Array(items) => stack.extend(items.iter()),
            Value::Object(map) => {
                for (k, item) in map {
                    bytes += k.len();
                    stack.push(item);
                }
            }
            Value::String(s) => bytes += s.len(),
            _ => bytes += 8,
        }
    }
    (nodes, bytes)
}

/// The tree colours of `p`.
pub fn json_visuals(p: &ChatPalette) -> JsonTreeVisuals {
    JsonTreeVisuals {
        object_key_color: p.json.key,
        array_idx_color: p.json.index,
        null_color: p.json.null,
        bool_color: p.json.boolean,
        number_color: p.json.number,
        string_color: p.json.string,
        highlight_color: p.json.highlight,
        punctuation_color: p.json.punct,
    }
}

/// What the user chose from the tree header.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
enum Expand {
    #[default]
    Role,
    All,
    Top,
}

#[derive(Clone, Copy, Debug, Default)]
struct TreeMemory {
    expand: Expand,
    as_text: bool,
    fingerprint: (usize, usize),
    nodes: usize,
    bytes: usize,
    /// A touch drag taken from a tree label scrolls the enclosing scroll area.
    scrolling: bool,
}

/// What a payload tree frame produced.
#[derive(Clone, Debug, Default)]
pub struct PayloadTreeResponse {
    /// Pretty JSON placed on the clipboard.
    pub copied: Option<String>,
}

/// A JSON payload with Expand all, Collapse and Copy, in its own id scope.
pub struct PayloadTree<'a> {
    id: Id,
    value: &'a Value,
    role: PayloadRole,
    budget: TreeBudget,
    touch: Option<bool>,
    word_wrap: bool,
    block_strings: Option<usize>,
    counts: bool,
}

impl<'a> PayloadTree<'a> {
    /// `id` scopes the expansion state; one id must not be drawn twice in a frame.
    pub fn new(id: Id, value: &'a Value, role: PayloadRole) -> Self {
        Self {
            id,
            value,
            role,
            budget: TreeBudget::default(),
            touch: None,
            word_wrap: true,
            block_strings: None,
            counts: false,
        }
    }

    /// Wraps strings at word boundaries, splitting only runs wider than the row; `false` breaks them anywhere. Defaults to `true`.
    pub fn word_wrap(mut self, on: bool) -> Self {
        self.word_wrap = on;
        self
    }

    /// Draws a string longer than `chars` characters, or holding a newline, indented on its own line under its key at full width.
    pub fn block_strings(mut self, chars: usize) -> Self {
        self.block_strings = Some(chars);
        self
    }

    /// Reads a collapsed object or array as `{N}` or `[N]`, its entry count, instead of `{...}`.
    pub fn counts(mut self, on: bool) -> Self {
        self.counts = on;
        self
    }

    pub fn budget(mut self, budget: TreeBudget) -> Self {
        self.budget = budget;
        self
    }

    /// Forces click-only tokens on or off; defaults to whether the screen is touched.
    pub fn touch(mut self, touch: bool) -> Self {
        self.touch = Some(touch);
        self
    }

    pub fn show(self, ui: &mut Ui) -> PayloadTreeResponse {
        let Self {
            id,
            value,
            role,
            budget,
            touch,
            word_wrap,
            block_strings,
            counts,
        } = self;
        let mem_id = id.with("egui_theme_kit.chat.payload");
        let mut mem = ui
            .ctx()
            .data(|d| d.get_temp::<TreeMemory>(mem_id))
            .unwrap_or_default();
        let fingerprint = (std::ptr::from_ref(value) as usize, top_len(value));
        if mem.fingerprint != fingerprint || mem.nodes == 0 {
            let (nodes, bytes) = count_nodes(value, budget.max_expand_nodes);
            mem.fingerprint = fingerprint;
            mem.nodes = nodes;
            mem.bytes = bytes;
        }
        let huge = mem.nodes > budget.max_expand_nodes;
        let lazy = mem.nodes > budget.lazy_nodes || mem.bytes > budget.lazy_bytes;
        let p = ChatPalette::of(ui);
        let icons = ChatIcons::get(ui.ctx());
        let touch = touch.unwrap_or_else(|| ui.input(|i| i.has_touch_screen()));
        let mut out = PayloadTreeResponse::default();
        let mut picked = None;

        let scope = ui.scope_builder(UiBuilder::new().scope_id(id), |ui| {
            ui.horizontal(|ui| {
                let expand_all = IconButton::new(icons.expand, IconBox::Sm)
                    .tip("Expand all")
                    .selected(mem.expand == Expand::All)
                    .enabled(!huge)
                    .show(ui);
                if expand_all.clicked() {
                    picked = Some(Expand::All);
                }
                let collapse = IconButton::new(icons.collapse, IconBox::Sm)
                    .tip("Collapse")
                    .selected(mem.expand == Expand::Top)
                    .show(ui);
                if collapse.clicked() {
                    picked = Some(Expand::Top);
                }
                if IconButton::new(icons.copy, IconBox::Sm)
                    .tip("Copy")
                    .show(ui)
                    .clicked()
                {
                    let text = pretty(value);
                    ui.ctx().copy_text(text.clone());
                    out.copied = Some(text);
                }
                if huge {
                    let label = if mem.as_text {
                        "Show as tree"
                    } else {
                        "Show as text"
                    };
                    if ui.link(label).clicked() {
                        mem.as_text = !mem.as_text;
                    }
                }
            });

            if huge && mem.as_text {
                show_text(ui, id, value, &p);
                return;
            }

            let repeat = picked.is_some_and(|e| e == mem.expand);
            if let Some(e) = picked {
                mem.expand = e;
            }
            let default_expand = match mem.expand {
                Expand::All if !huge => DefaultExpand::All,
                Expand::Top => DefaultExpand::ToLevel(0),
                _ if lazy => DefaultExpand::ToLevel(0),
                _ => DefaultExpand::ToLevel(role.level()),
            };
            let look = Look {
                visuals: json_visuals(&p),
                shell: p.shell,
                font: TextStyle::Monospace.resolve(ui.style()),
                touch,
                word_wrap,
                block_strings,
                counts,
            };
            let style = JsonTreeStyle::new()
                .visuals(look.visuals.clone())
                .font_id(look.font.clone());
            let mut keyed = false;
            let tree = JsonTree::new(id.with("tree"), value)
                .style(style)
                .default_expand(default_expand)
                .on_render(move |ui, ctx| render_token(ui, ctx, &look, &mut keyed));
            let response = tree.show(ui);
            if repeat {
                response.reset_expanded(ui);
                ui.ctx().request_repaint();
            }
        });
        if touch {
            scroll_label_drags(ui, scope.response.rect, &mut mem.scrolling);
        }

        ui.ctx().data_mut(|d| d.insert_temp(mem_id, mem));
        out
    }
}

fn top_len(value: &Value) -> usize {
    match value {
        Value::Array(a) => a.len(),
        Value::Object(o) => o.len(),
        Value::String(s) => s.len(),
        _ => 0,
    }
}

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_default()
}

/// Pretty JSON in a wrapped monospace label, built once per id.
fn show_text(ui: &mut Ui, id: Id, value: &Value, p: &ChatPalette) {
    let text_id = id.with("egui_theme_kit.chat.payload_text");
    let text = ui
        .ctx()
        .data(|d| d.get_temp::<Arc<String>>(text_id))
        .unwrap_or_else(|| {
            let text = Arc::new(pretty(value));
            ui.ctx().data_mut(|d| d.insert_temp(text_id, text.clone()));
            text
        });
    let font = TextStyle::Monospace.resolve(ui.style());
    let mut job =
        LayoutJob::single_section(text.as_str().to_owned(), TextFormat::simple(font, p.text));
    job.wrap.max_width = ui.available_width();
    job.wrap.break_anywhere = true;
    ui.add(Label::new(job).sense(Sense::click()));
}

/// How tree tokens are drawn.
struct Look {
    visuals: JsonTreeVisuals,
    shell: ShellColors,
    font: egui::FontId,
    /// Every token is click-only.
    touch: bool,
    word_wrap: bool,
    block_strings: Option<usize>,
    counts: bool,
}

impl Look {
    fn format(&self, color: egui::Color32) -> TextFormat {
        TextFormat::simple(self.font.clone(), color)
    }

    /// Whether string `s` drawn after its key moves to its own line.
    fn blocks(&self, s: &str) -> bool {
        self.block_strings
            .is_some_and(|n| s.contains('\n') || s.chars().nth(n).is_some())
    }
}

/// Draws a tree token: strings wrap, command strings are shell-coloured, on touch every token is click-only; `keyed` marks a value's key.
fn render_token(ui: &mut Ui, ctx: RenderContext<'_, '_, Value>, look: &Look, keyed: &mut bool) {
    let after_key = std::mem::replace(
        keyed,
        matches!(&ctx, RenderContext::Property(c) if c.collapsing_state.is_none()),
    );
    let visuals = &look.visuals;
    let (mut job, break_anywhere) = match &ctx {
        RenderContext::BaseValue(c) if c.value_type == BaseValueType::String => {
            let command = matches!(
                c.pointer.last(),
                Some(JsonPointerSegment::Key(k)) if is_command_key(k)
            );
            let shown = c.display_value.to_string();
            let quote = look.format(visuals.get_color(&c.value_type));
            let mut job = LayoutJob::default();
            if command {
                job.append("\"", 0.0, quote.clone());
                super::shell::append(&mut job, &shown, &look.font, &look.shell);
                job.append("\"", 0.0, quote);
            } else {
                job.append(&format!("\"{shown}\""), 0.0, quote);
            }
            if after_key && look.blocks(&shown) {
                job.wrap.break_anywhere = !look.word_wrap;
                block_token(ui, job);
                return;
            }
            (job, !look.word_wrap)
        }
        RenderContext::ExpandableDelimiter(c) if look.counts && collapsed(c.delimiter) => {
            let n = match c.value {
                Value::Array(items) => items.len(),
                Value::Object(map) => map.len(),
                _ => 0,
            };
            let text = match c.delimiter {
                ExpandableDelimiter::CollapsedArray => format!("[{n}]"),
                _ => format!("{{{n}}}"),
            };
            let job = LayoutJob::single_section(text, look.format(visuals.punctuation_color));
            (job, true)
        }
        _ if !look.touch => {
            ctx.render_default(ui);
            return;
        }
        RenderContext::Property(c) => {
            let (text, color) = match &c.property {
                JsonPointerSegment::Index(i) => (i.to_string(), visuals.array_idx_color),
                JsonPointerSegment::Key(k) => (format!("\"{k}\""), visuals.object_key_color),
            };
            (LayoutJob::single_section(text, look.format(color)), true)
        }
        RenderContext::BaseValue(c) => (
            LayoutJob::single_section(
                c.display_value.to_string(),
                look.format(visuals.get_color(&c.value_type)),
            ),
            true,
        ),
        RenderContext::ExpandableDelimiter(c) => (
            LayoutJob::single_section(
                c.delimiter.as_ref().to_owned(),
                look.format(visuals.punctuation_color),
            ),
            true,
        ),
    };
    job.wrap.max_width = token_wrap_width(ui);
    job.wrap.break_anywhere = break_anywhere;
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    ui.add(Label::new(galley).sense(Sense::click()));
}

/// Whether `delimiter` stands for a collapsed, non-empty object or array.
fn collapsed(delimiter: ExpandableDelimiter) -> bool {
    matches!(
        delimiter,
        ExpandableDelimiter::CollapsedArray | ExpandableDelimiter::CollapsedObject
    )
}

/// Draws `job` under the key that opens the current row, indented one level and as wide as the row.
fn block_token(ui: &mut Ui, mut job: LayoutJob) {
    let row = ui.min_rect();
    let left = ui.max_rect().left() + ui.spacing().indent;
    let right = ui.max_rect().right().min(ui.clip_rect().right());
    job.wrap.max_width = (right - left).max(MIN_TOKEN_W);
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    let rect = Rect::from_min_size(pos2(left, row.bottom()), galley.size());
    let builder = UiBuilder::new()
        .max_rect(rect)
        .layout(Layout::top_down(Align::Min));
    ui.scope_builder(builder, |ui| {
        ui.add(Label::new(galley).sense(Sense::click()));
    });
}

/// Stops drags held by labels inside `rect` and scrolls the enclosing scroll area with the finger instead.
fn scroll_label_drags(ui: &Ui, rect: Rect, scrolling: &mut bool) {
    let ctx = ui.ctx();
    let inside = ctx.dragged_id().is_some_and(|id| {
        ctx.viewport(|v| {
            v.this_pass
                .widgets
                .get(id)
                .is_some_and(|w| rect.expand(1.0).contains_rect(w.interact_rect))
        })
    });
    if inside {
        ctx.stop_dragging();
        *scrolling = true;
    }
    let (down, delta) = ctx.input(|i| (i.pointer.primary_down(), i.pointer.delta()));
    if !down {
        *scrolling = false;
    } else if *scrolling && delta.y != 0.0 {
        ui.scroll_with_delta_animation(vec2(0.0, delta.y), ScrollAnimation::none());
    }
}

/// Narrowest wrap width of a token.
const MIN_TOKEN_W: f32 = 48.0;

/// Width left for a token before the visible right edge.
fn token_wrap_width(ui: &Ui) -> f32 {
    let left = ui.available_rect_before_wrap().left();
    let right = ui.max_rect().right().min(ui.clip_rect().right());
    (right - left).max(MIN_TOKEN_W)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn counting_stops_past_the_limit() {
        let value = json!({ "a": [1, 2, 3], "b": "xyz" });
        assert_eq!(count_nodes(&value, 100), (6, 3 + 8 * 3 + 2));
        let wide = Value::Array((0..50).map(Value::from).collect());
        assert_eq!(count_nodes(&wide, 10).0, 11);
    }

    #[test]
    fn roles_open_to_their_depth() {
        assert_eq!(PayloadRole::Arguments.level(), 2);
        assert_eq!(PayloadRole::Result.level(), 1);
        assert_eq!(PayloadRole::Depth(4).level(), 4);
    }

    #[test]
    fn a_tree_draws_with_and_without_touch() {
        let ctx = egui::Context::default();
        let value = json!({ "cmd": "ls -la", "n": [1, 2, { "deep": true }] });
        for touch in [false, true] {
            for _ in 0..2 {
                let mut out = ctx.run_ui(Default::default(), |ui| {
                    let id = Id::unique(("payload", touch));
                    let out = PayloadTree::new(id, &value, PayloadRole::Arguments)
                        .touch(touch)
                        .show(ui);
                    assert!(out.copied.is_none());
                });
                out.textures_delta.clear();
            }
        }
    }

    #[test]
    fn long_strings_wrap_inside_the_available_width() {
        let ctx = egui::Context::default();
        let long = "assertion failed: response.rect ".repeat(12);
        let value = json!({ "message": long, "filter": "transcript::follows_bottom_after_a_send" });
        for touch in [false, true] {
            let mut width = 0.0;
            for _ in 0..3 {
                let mut out = ctx.run_ui(Default::default(), |ui| {
                    let inner = ui.allocate_ui(egui::vec2(220.0, 2000.0), |ui| {
                        PayloadTree::new(Id::unique(("wrap", touch)), &value, PayloadRole::Result)
                            .touch(touch)
                            .show(ui);
                    });
                    width = inner.response.rect.width();
                });
                out.textures_delta.clear();
            }
            assert!(width <= 221.0, "touch {touch}: tree is {width} wide");
        }
    }

    /// Every text the tree `setup` builds paints in a column `width` wide, with its galley rect.
    fn tree_texts(
        value: &Value,
        role: PayloadRole,
        width: f32,
        touch: bool,
        setup: impl Fn(PayloadTree<'_>) -> PayloadTree<'_>,
    ) -> Vec<(String, Rect, Arc<egui::Galley>)> {
        let ctx = egui::Context::default();
        let mut texts = Vec::new();
        for _ in 0..3 {
            let mut out = ctx.run_ui(Default::default(), |ui| {
                ui.allocate_ui(egui::vec2(width, 4000.0), |ui| {
                    let tree = PayloadTree::new(Id::unique("texts"), value, role);
                    setup(tree.touch(touch)).show(ui);
                });
            });
            out.textures_delta.clear();
            texts = out
                .shapes
                .iter()
                .filter_map(|c| match &c.shape {
                    egui::Shape::Text(t) => Some((
                        t.galley.text().to_owned(),
                        Rect::from_min_size(t.pos, t.galley.size()),
                        t.galley.clone(),
                    )),
                    _ => None,
                })
                .collect();
        }
        texts
    }

    fn find<'t>(
        texts: &'t [(String, Rect, Arc<egui::Galley>)],
        text: &str,
    ) -> &'t (String, Rect, Arc<egui::Galley>) {
        texts
            .iter()
            .find(|(t, ..)| t == text)
            .unwrap_or_else(|| panic!("{text:?} was not painted"))
    }

    /// Character offsets where each row of `galley` after the first starts.
    fn row_starts(galley: &egui::Galley) -> Vec<usize> {
        let mut at = 0;
        let mut starts = Vec::new();
        for row in &galley.rows {
            if at > 0 {
                starts.push(at);
            }
            at += row.glyphs.len() + usize::from(row.ends_with_newline);
        }
        starts
    }

    #[test]
    fn strings_wrap_at_words_unless_word_wrap_is_off() {
        let text = "lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod tempor";
        let value = json!({ "k": text });
        let quoted = format!("\"{text}\"");
        let chars: Vec<char> = quoted.chars().collect();
        let at_word = |p: usize| chars[p - 1] == ' ' || chars[p] == ' ';
        for touch in [false, true] {
            let words = tree_texts(&value, PayloadRole::Result, 200.0, touch, |t| t);
            let starts = row_starts(&find(&words, &quoted).2);
            assert!(starts.len() > 1, "touch {touch}: no wrap");
            assert!(
                starts.iter().all(|&p| at_word(p)),
                "touch {touch}: {starts:?}"
            );

            let anywhere = tree_texts(&value, PayloadRole::Result, 200.0, touch, |t| {
                t.word_wrap(false)
            });
            let starts = row_starts(&find(&anywhere, &quoted).2);
            assert!(
                starts.iter().any(|&p| !at_word(p)),
                "touch {touch}: {starts:?}"
            );
        }
    }

    #[test]
    fn an_unbroken_run_still_splits_with_word_wrap() {
        let run = "x".repeat(160);
        let value = json!({ "k": run });
        let texts = tree_texts(&value, PayloadRole::Result, 220.0, false, |t| t);
        let (_, rect, galley) = find(&texts, &format!("\"{run}\""));
        assert!(galley.rows.len() > 1);
        assert!(rect.right() <= 221.0, "{rect:?}");
    }

    #[test]
    fn block_strings_draw_under_their_key_at_full_width() {
        let long = "assertion failed: response.rect contains the pointer ".repeat(3);
        let value = json!({ "message": long, "note": "one\ntwo", "short": "ok" });
        for touch in [false, true] {
            let texts = tree_texts(&value, PayloadRole::Result, 320.0, touch, |t| {
                t.block_strings(40)
            });
            let key = |k: &str| find(&texts, &format!("\"{k}\"")).1;
            let message = find(&texts, &format!("\"{long}\"")).1;
            assert!(
                message.top() >= key("message").bottom() - 0.5,
                "touch {touch}"
            );
            assert!(
                message.left() > key("message").left(),
                "touch {touch}: not indented"
            );
            assert!(
                message.left() < key("message").right(),
                "touch {touch}: not under the key"
            );
            assert!(
                message.width() > 320.0 - message.left() - 30.0,
                "touch {touch}: {message:?}"
            );
            assert!(message.right() <= 321.0, "touch {touch}: {message:?}");
            let note = find(&texts, "\"one\ntwo\"").1;
            assert!(note.top() >= key("note").bottom() - 0.5, "touch {touch}");
            assert!(key("note").top() >= message.bottom() - 0.5, "touch {touch}");
            assert!(key("short").top() >= note.bottom() - 0.5, "touch {touch}");
            let ok = find(&texts, "\"ok\"").1;
            assert!(
                (ok.center().y - key("short").center().y).abs() < 1.0,
                "touch {touch}"
            );

            let inline = tree_texts(&value, PayloadRole::Result, 320.0, touch, |t| t);
            let key = find(&inline, "\"message\"").1;
            let message = find(&inline, &format!("\"{long}\"")).1;
            assert!(message.left() >= key.right(), "touch {touch}: {message:?}");
        }
    }

    #[test]
    fn counts_label_collapsed_objects_and_arrays() {
        let value = json!({ "a": { "x": 1, "y": 2 }, "b": [1, 2, 3], "c": {}, "d": [] });
        for touch in [false, true] {
            let shown = |counts: bool| {
                tree_texts(&value, PayloadRole::Depth(0), 400.0, touch, |t| {
                    t.counts(counts)
                })
                .into_iter()
                .map(|(t, ..)| t)
                .collect::<Vec<_>>()
            };
            let counted = shown(true);
            for text in ["{2}", "[3]", "{}", "[]"] {
                assert!(
                    counted.iter().any(|t| t == text),
                    "touch {touch}: {text} in {counted:?}"
                );
            }
            assert!(
                !counted.iter().any(|t| t.contains("...")),
                "touch {touch}: {counted:?}"
            );
            let plain = shown(false);
            for text in ["{...}", "[...]"] {
                assert!(
                    plain.iter().any(|t| t == text),
                    "touch {touch}: {text} in {plain:?}"
                );
            }
        }
    }

    #[test]
    fn a_touch_drag_on_a_colon_scrolls_the_enclosing_area() {
        use egui::{
            Event, Modifiers, PointerButton, Pos2, RawInput, Rect, ScrollArea, TouchDeviceId,
            TouchId, TouchPhase,
        };
        let ctx = egui::Context::default();
        let value = Value::Object(
            (0..40)
                .map(|n| (format!("key{n:02}"), Value::from(n)))
                .collect(),
        );
        let finger = |phase, pos| Event::Touch {
            device_id: TouchDeviceId(1),
            id: TouchId(1),
            phase,
            pos,
            force: None,
        };
        let mut time = 0.0;
        let mut frame = |events: Vec<Event>| {
            time += 1.0 / 60.0;
            let raw = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 300.0))),
                time: Some(time),
                events,
                ..RawInput::default()
            };
            let mut offset = 0.0;
            let mut out = ctx.run_ui(raw, |ui| {
                offset = ScrollArea::vertical()
                    .max_height(200.0)
                    .show(ui, |ui| {
                        PayloadTree::new(Id::unique("colons"), &value, PayloadRole::Result)
                            .touch(true)
                            .show(ui);
                    })
                    .state
                    .clamped_offset()
                    .y;
            });
            out.textures_delta.clear();
            let colon = out.shapes.iter().find_map(|c| match &c.shape {
                egui::Shape::Text(t) if t.galley.text().starts_with(':') => {
                    let rect = Rect::from_min_size(t.pos, t.galley.size());
                    (rect.center().y > 40.0 && rect.center().y < 160.0).then_some(rect.center())
                }
                _ => None,
            });
            (offset, colon)
        };
        frame(vec![finger(TouchPhase::Cancel, Pos2::ZERO)]);
        let (_, colon) = frame(Vec::new());
        let p = colon.expect("a colon is drawn");
        frame(vec![
            finger(TouchPhase::Start, p),
            Event::PointerMoved(p),
            Event::PointerButton {
                pos: p,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ]);
        let mut offset = 0.0;
        for dy in [20.0, 40.0, 60.0, 60.0] {
            let q = p - egui::vec2(0.0, dy);
            offset = frame(vec![finger(TouchPhase::Move, q), Event::PointerMoved(q)]).0;
        }
        assert!(offset > 30.0, "{offset}");
    }
}
