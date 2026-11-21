//! Headless frames of the chat demo; `write_previews` rasterises them to PNG files in `CHAT_PREVIEW_DIR`.

#[path = "../examples/chat_demo/demo.rs"]
mod demo;

use std::collections::HashMap;
use std::path::Path;

use egui::epaint::{ClippedPrimitive, ClippedShape, Primitive, Vertex};
use egui::{
    Color32, ColorImage, Context, Event, FontDefinitions, ImageData, Key, Modifiers, PointerButton,
    Pos2, RawInput, Rect, Shape, Style, TextureId, TouchDeviceId, TouchId, TouchPhase, Vec2,
    Visuals, pos2, vec2,
};
use egui_theme_kit::chat::{ChatPalette, Composer, UnixMs};
use egui_theme_kit::{Density, PresetStyles, Tokens};

const NOW: UnixMs = UnixMs(1_763_650_000_000);
const COMPOSER: &str = "chat_demo.composer";
const WIDE: Vec2 = Vec2::new(1200.0, 2300.0);
const NARROW: Vec2 = Vec2::new(380.0, 3300.0);

#[derive(Clone)]
enum Theme {
    Preset(PresetStyles),
    /// egui's light visuals, optionally with another accent.
    Light(Option<Color32>),
}

impl Theme {
    fn name(&self) -> String {
        match self {
            Self::Preset(p) => p.id().unwrap_or("custom").to_owned(),
            Self::Light(None) => "egui-light".to_owned(),
            Self::Light(Some(_)) => "egui-light-teal".to_owned(),
        }
    }

    fn apply(&self, ctx: &Context) {
        match self {
            Self::Preset(p) => {
                egui_theme_kit::apply_preset(ctx, *p);
            }
            Self::Light(accent) => {
                let mut visuals = Visuals::light();
                if let Some(accent) = accent {
                    visuals.selection.bg_fill = *accent;
                    visuals.hyperlink_color = *accent;
                }
                egui_theme_kit::metrics::install_overlaid_style(
                    ctx,
                    Style {
                        visuals,
                        ..Style::default()
                    },
                );
            }
        }
    }

    fn preset(&self) -> PresetStyles {
        match self {
            Self::Preset(p) => *p,
            Self::Light(_) => PresetStyles::DefaultEgui,
        }
    }
}

fn context(theme: &Theme) -> Context {
    let ctx = Context::default();
    let mut fonts = FontDefinitions::default();
    egui_theme_kit::fonts::install(&mut fonts);
    ctx.set_fonts(fonts);
    egui_theme_kit::set_density(&ctx, Density::Regular);
    theme.apply(&ctx);
    ctx
}

fn is_dark(theme: &Theme) -> bool {
    ChatPalette::get(&context(theme)).dark
}

/// Two dark presets, then light presets, topped up with egui's light visuals.
fn preview_themes() -> Vec<Theme> {
    let mut themes = vec![
        Theme::Preset(PresetStyles::Graphite),
        Theme::Preset(PresetStyles::Frost),
    ];
    let light: Vec<Theme> = egui_theme_kit::presets::MATCHABLE_PRESETS
        .into_iter()
        .map(Theme::Preset)
        .filter(|t| !is_dark(t))
        .take(2)
        .collect();
    themes.extend(light);
    let teal = Color32::from_rgb(0, 128, 128);
    for extra in [Theme::Light(None), Theme::Light(Some(teal))] {
        if themes.len() < 4 {
            themes.push(extra);
        }
    }
    themes
}

fn key(key: Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

fn click(pos: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

/// Every text shape with its screen rect.
fn texts(shapes: &[ClippedShape]) -> Vec<(String, Rect)> {
    fn walk(shape: &Shape, out: &mut Vec<(String, Rect)>) {
        match shape {
            Shape::Text(t) => out.push((
                t.galley.text().to_owned(),
                t.galley.rect.translate(t.pos.to_vec2()),
            )),
            Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for s in shapes {
        walk(&s.shape, &mut out);
    }
    out
}

/// One frame's output plus the font atlas so far.
struct Frame {
    shapes: Vec<ClippedShape>,
    ppp: f32,
}

/// Drives the demo through frames and keeps the font atlas in sync.
struct Harness {
    ctx: Context,
    demo: demo::ChatDemo,
    size: Vec2,
    touch: bool,
    textures: HashMap<TextureId, ColorImage>,
}

impl Harness {
    fn new(theme: &Theme, size: Vec2, touch: bool) -> Self {
        Self {
            ctx: context(theme),
            demo: demo::ChatDemo::new(NOW, theme.preset()),
            size,
            touch,
            textures: HashMap::new(),
        }
    }

    fn frame(&mut self, mut events: Vec<Event>) -> Frame {
        if self.touch {
            events.insert(
                0,
                Event::Touch {
                    device_id: TouchDeviceId(1),
                    id: TouchId(1),
                    phase: TouchPhase::Cancel,
                    pos: Pos2::ZERO,
                    force: None,
                },
            );
        }
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
            events,
            ..RawInput::default()
        };
        let demo = &mut self.demo;
        demo.set_now(NOW);
        let mut full = self.ctx.run_ui(input, |ui| demo.show(ui));
        for (id, deltas) in full.textures_delta.set.drain() {
            for delta in deltas {
                let ImageData::Color(image) = delta.image;
                match delta.pos {
                    None => {
                        self.textures.insert(id, (*image).clone());
                    }
                    Some([x0, y0]) => {
                        if let Some(tex) = self.textures.get_mut(&id) {
                            let w = tex.size[0];
                            for y in 0..image.size[1] {
                                let row = y * image.size[0];
                                let at = (y0 + y) * w + x0;
                                tex.pixels[at..at + image.size[0]]
                                    .copy_from_slice(&image.pixels[row..row + image.size[0]]);
                            }
                        }
                    }
                }
            }
        }
        Frame {
            shapes: std::mem::take(&mut full.shapes),
            ppp: full.pixels_per_point,
        }
    }

    fn idle(&mut self, frames: usize) -> Frame {
        let mut last = self.frame(vec![Event::PointerGone]);
        for _ in 1..frames {
            last = self.frame(Vec::new());
        }
        last
    }

    /// Settles the layout, opens the failed tool call, then opens the slash menu in the composer.
    fn stage(&mut self) -> Frame {
        let settled = self.idle(8);
        let call = texts(&settled.shapes)
            .into_iter()
            .find(|(t, _)| t.starts_with("run_tests"))
            .map(|(_, r)| r.center());
        let pos = call.expect("the failed tool call is drawn");
        self.frame(vec![Event::PointerMoved(pos)]);
        self.frame(vec![click(pos, true)]);
        self.frame(vec![click(pos, false)]);
        self.idle(4);
        let id = Composer::text_id(COMPOSER);
        self.ctx.memory_mut(|m| m.request_focus(id));
        self.idle(2);
        let start = Modifiers {
            ctrl: true,
            command: true,
            ..Modifiers::NONE
        };
        self.frame(vec![key(Key::Home, start)]);
        self.frame(vec![key(Key::ArrowRight, Modifiers::NONE)]);
        self.idle(4)
    }
}

/// Fails when the staged demo lacks one of its widgets.
fn assert_shows_every_widget(frame: &Frame, label: &str, wide: bool) {
    let all: Vec<String> = texts(&frame.shapes).into_iter().map(|(t, _)| t).collect();
    let joined = all.join(
        "
",
    );
    let wide_only: &[&str] = if wide {
        &["Your messages", "Thinking", "Today"]
    } else {
        &[]
    };
    let always = [
        "Chats",
        "Fix the flaky transcript scroll test",
        "What I found",
        "Worked",
        "run_tests",
        "first_failure",
        "Fix plan",
        "Run a command",
        "Before I commit",
        "Release notes draft",
        "compact",
    ];
    for needle in wide_only.iter().copied().chain(always) {
        assert!(
            joined.contains(needle),
            "{label}: no text containing {needle:?}"
        );
    }
}

#[test]
fn demo_shows_every_widget_wide_and_narrow() {
    for theme in [Theme::Preset(PresetStyles::Graphite), Theme::Light(None)] {
        for (size, touch) in [(WIDE, false), (NARROW, true)] {
            let mut h = Harness::new(&theme, size, touch);
            let frame = h.stage();
            let label = format!("{} {}", theme.name(), size.x);
            assert_shows_every_widget(&frame, &label, !touch);
        }
    }
}

#[test]
#[ignore = "writes PNG files; set CHAT_PREVIEW_DIR"]
fn write_previews() {
    let dir = std::env::var("CHAT_PREVIEW_DIR").expect("CHAT_PREVIEW_DIR names the output folder");
    let dir = Path::new(&dir);
    std::fs::create_dir_all(dir).expect("output folder");
    for theme in preview_themes() {
        for (size, touch) in [(WIDE, false), (NARROW, true)] {
            let mut h = Harness::new(&theme, size, touch);
            let frame = h.stage();
            let ctx = h.ctx.clone();
            let prims = ctx.tessellate(frame.shapes, frame.ppp);
            let base = Tokens::get(&ctx).base;
            let (w, hgt) = ((size.x * frame.ppp) as usize, (size.y * frame.ppp) as usize);
            let mut buf = vec![base.to_normalized_gamma_f32(); w * hgt];
            raster(&mut buf, w, hgt, frame.ppp, &prims, &h.textures);
            let name = format!("chat_{}_{}.png", theme.name(), size.x as u32);
            write_png(&dir.join(&name), w, hgt, &buf);
            println!("{} dark={}", dir.join(&name).display(), is_dark(&theme));
        }
    }
}

fn write_png(path: &Path, w: usize, h: usize, buf: &[[f32; 4]]) {
    let file = std::fs::File::create(path).expect("png file");
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), w as u32, h as u32);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("png header");
    let data: Vec<u8> = buf
        .iter()
        .flat_map(|p| {
            p[..3]
                .iter()
                .map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8)
        })
        .collect();
    writer.write_image_data(&data).expect("png data");
}

fn raster(
    buf: &mut [[f32; 4]],
    w: usize,
    h: usize,
    ppp: f32,
    prims: &[ClippedPrimitive],
    textures: &HashMap<TextureId, ColorImage>,
) {
    for prim in prims {
        let Primitive::Mesh(mesh) = &prim.primitive else {
            continue;
        };
        let tex = textures.get(&mesh.texture_id);
        let clip = Rect::from_min_max(
            pos2(prim.clip_rect.min.x * ppp, prim.clip_rect.min.y * ppp),
            pos2(prim.clip_rect.max.x * ppp, prim.clip_rect.max.y * ppp),
        );
        for tri in mesh.indices.as_chunks::<3>().0 {
            let v = tri.map(|i| mesh.vertices[i as usize]);
            fill(buf, (w, h), clip, ppp, v, tex);
        }
    }
}

fn cross(a: Vec2, b: Vec2, c: Vec2) -> f32 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

/// Blends one textured, vertex-coloured triangle over `buf` with premultiplied alpha.
fn fill(
    buf: &mut [[f32; 4]],
    (w, h): (usize, usize),
    clip: Rect,
    ppp: f32,
    mut v: [Vertex; 3],
    tex: Option<&ColorImage>,
) {
    let mut p = v.map(|v| v.pos.to_vec2() * ppp);
    let mut area = cross(p[0], p[1], p[2]);
    if area.abs() < 1e-9 {
        return;
    }
    if area < 0.0 {
        p.swap(1, 2);
        v.swap(1, 2);
        area = -area;
    }
    let lo = vec2(
        p.iter()
            .map(|q| q.x)
            .fold(f32::INFINITY, f32::min)
            .max(clip.min.x)
            .max(0.0),
        p.iter()
            .map(|q| q.y)
            .fold(f32::INFINITY, f32::min)
            .max(clip.min.y)
            .max(0.0),
    );
    let hi = vec2(
        p.iter()
            .map(|q| q.x)
            .fold(f32::NEG_INFINITY, f32::max)
            .min(clip.max.x)
            .min(w as f32),
        p.iter()
            .map(|q| q.y)
            .fold(f32::NEG_INFINITY, f32::max)
            .min(clip.max.y)
            .min(h as f32),
    );
    if lo.x >= hi.x || lo.y >= hi.y {
        return;
    }
    let edges = [(p[1], p[2]), (p[2], p[0]), (p[0], p[1])];
    for py in lo.y.floor() as usize..(hi.y.ceil() as usize).min(h) {
        for px in lo.x.floor() as usize..(hi.x.ceil() as usize).min(w) {
            let q = vec2(px as f32 + 0.5, py as f32 + 0.5);
            if q.x < clip.min.x || q.x >= clip.max.x || q.y < clip.min.y || q.y >= clip.max.y {
                continue;
            }
            let e = edges.map(|(a, b)| cross(a, b, q));
            if e.iter().any(|&e| e < 0.0) {
                continue;
            }
            let bary = e.map(|e| e / area);
            let mut src = [0.0_f32; 4];
            for (k, out) in src.iter_mut().enumerate() {
                *out = v
                    .iter()
                    .zip(bary)
                    .map(|(vv, b)| f32::from(vv.color.to_array()[k]) * b)
                    .sum::<f32>()
                    / 255.0;
            }
            if let Some(t) = tex {
                let u: f32 = v.iter().zip(bary).map(|(vv, b)| vv.uv.x * b).sum();
                let s: f32 = v.iter().zip(bary).map(|(vv, b)| vv.uv.y * b).sum();
                let (tw, th) = (t.size[0], t.size[1]);
                let x = ((u * tw as f32) as usize).min(tw - 1);
                let y = ((s * th as f32) as usize).min(th - 1);
                let c = t.pixels[y * tw + x].to_array();
                for (k, out) in src.iter_mut().enumerate() {
                    *out *= f32::from(c[k]) / 255.0;
                }
            }
            let d = &mut buf[py * w + px];
            for k in 0..4 {
                d[k] = src[k] + d[k] * (1.0 - src[3]);
            }
        }
    }
}
