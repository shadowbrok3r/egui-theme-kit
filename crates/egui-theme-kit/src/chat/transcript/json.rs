//! Tool payloads as `egui_json_tree` trees, sized and coloured for chat.

use std::sync::Arc;

use egui::{Id, Label, Response, Sense, TextFormat, TextStyle, Ui, UiBuilder, text::LayoutJob};
use egui_json_tree::{
    DefaultExpand, JsonTree, JsonTreeStyle, JsonTreeVisuals, pointer::JsonPointerSegment,
    render::RenderContext, value::BaseValueType,
};
use serde_json::Value;

use crate::chat::icons::ChatIcons;
use crate::chat::palette::ChatPalette;
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
        }
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

        ui.scope_builder(UiBuilder::new().scope_id(id), |ui| {
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
            let visuals = json_visuals(&p);
            let font = TextStyle::Monospace.resolve(ui.style());
            let style = JsonTreeStyle::new()
                .visuals(visuals.clone())
                .font_id(font.clone());
            let tree = JsonTree::new(id.with("tree"), value)
                .style(style)
                .default_expand(default_expand)
                .on_render_if(touch, move |ui, ctx| {
                    render_click_only(ui, ctx, &visuals, &font);
                });
            let response = tree.show(ui);
            if repeat {
                response.reset_expanded(ui);
                ui.ctx().request_repaint();
            }
        });

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

/// Draws a tree token as a click-only label so drags scroll the transcript on touch screens.
fn render_click_only(
    ui: &mut Ui,
    ctx: RenderContext<'_, '_, Value>,
    visuals: &JsonTreeVisuals,
    font: &egui::FontId,
) -> Response {
    let (text, color) = match &ctx {
        RenderContext::Property(c) => match &c.property {
            JsonPointerSegment::Index(i) => (i.to_string(), visuals.array_idx_color),
            JsonPointerSegment::Key(k) => (format!("\"{k}\""), visuals.object_key_color),
        },
        RenderContext::BaseValue(c) => {
            let shown = c.display_value.to_string();
            let text = if c.value_type == BaseValueType::String {
                format!("\"{shown}\"")
            } else {
                shown
            };
            (text, visuals.get_color(&c.value_type))
        }
        RenderContext::ExpandableDelimiter(c) => {
            (c.delimiter.as_ref().to_owned(), visuals.punctuation_color)
        }
    };
    let mut job = LayoutJob::single_section(text, TextFormat::simple(font.clone(), color));
    job.wrap.max_width = ui.available_width();
    job.wrap.break_anywhere = true;
    ui.add(Label::new(job).sense(Sense::click()))
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
}
