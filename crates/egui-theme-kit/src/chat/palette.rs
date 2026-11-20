//! Chat colours and radii derived from the kit [`Tokens`], cached per context while the tokens hold.

use egui::{Color32, Context, Id, Ui};

use crate::tokens::{Tokens, composite, contrast, ink_on, is_dark, mix};

/// Minimum contrast of body text on its fill.
pub const MIN_TEXT_CONTRAST: f32 = 4.5;
/// Minimum contrast of syntax and status colours on the panel.
pub const MIN_ACCENT_CONTRAST: f32 = 3.0;
/// Opacity of the sunken plate behind code blocks.
const PLATE_ALPHA: f32 = 0.6;
/// Opacity of the primary wash behind inline code.
const CODE_CHIP_ALPHA: f32 = 0.18;
/// Opacity of the primary wash behind search matches.
const HIGHLIGHT_ALPHA: f32 = 0.35;
/// Share of the tertiary hue in the user bubble edge.
const USER_STROKE_MIX: f32 = 0.35;
/// Step toward the ink when lifting a colour's contrast.
const LEGIBLE_STEP: f32 = 0.1;

/// Colours of JSON payload trees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JsonColors {
    pub key: Color32,
    pub string: Color32,
    pub number: Color32,
    pub boolean: Color32,
    pub null: Color32,
    pub index: Color32,
    /// Brackets, braces, colons and commas.
    pub punct: Color32,
    /// Background of search matches; translucent.
    pub highlight: Color32,
}

/// Colours of shell command highlighting, one per lexer class.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShellColors {
    pub command: Color32,
    pub flag: Color32,
    pub string: Color32,
    pub number: Color32,
    pub operator: Color32,
    /// Arguments and everything else.
    pub base: Color32,
}

/// Every colour and radius chat widgets paint with; plates may be translucent, the rest are opaque.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChatPalette {
    /// Whether the panel is dark.
    pub dark: bool,
    /// Panel background that translucent fills composite over.
    pub base: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub text_faint: Color32,
    pub link: Color32,
    pub accent: Color32,
    pub accent_soft: Color32,
    /// Text and icons on an `accent` fill.
    pub on_accent: Color32,
    pub user_fill: Color32,
    pub user_stroke: Color32,
    pub user_text: Color32,
    pub card_fill: Color32,
    pub card_stroke: Color32,
    /// Text on `card_fill`.
    pub card_text: Color32,
    pub popup_fill: Color32,
    pub sunken: Color32,
    /// Plate behind code blocks; translucent.
    pub code_plate: Color32,
    /// Wash behind inline code; translucent.
    pub code_chip: Color32,
    pub separator: Color32,
    pub chip_fill: Color32,
    pub chip_stroke: Color32,
    pub chip_text: Color32,
    pub mention_fill: Color32,
    pub mention_text: Color32,
    pub running: Color32,
    pub success: Color32,
    pub warn: Color32,
    pub error: Color32,
    pub info: Color32,
    pub json: JsonColors,
    pub shell: ShellColors,
    pub radius_bubble: u8,
    pub radius_card: u8,
    pub radius_chip: u8,
}

/// Builds a palette from tokens; apps replace the default with [`ChatPalette::set_builder`].
pub type PaletteBuilder = fn(&Tokens) -> ChatPalette;

#[derive(Clone, Copy)]
struct Cached {
    tokens: Tokens,
    generation: u64,
    palette: ChatPalette,
}

fn cache_id() -> Id {
    Id::unique("egui_theme_kit.chat.palette")
}

fn builder_id() -> Id {
    Id::unique("egui_theme_kit.chat.palette_builder")
}

impl ChatPalette {
    /// The default derivation; every colour comes from `t`.
    pub fn from_tokens(t: &Tokens) -> Self {
        let base = t.base;
        let accent_on_base = |c: Color32| legible(c, base, base, MIN_ACCENT_CONTRAST);
        let text = legible(
            legible(t.text, t.raised, base, MIN_TEXT_CONTRAST),
            base,
            base,
            MIN_TEXT_CONTRAST,
        );
        let user_fill = t.tertiary_soft;
        let mention_fill = t.primary_soft;
        let chip_fill = t.field;
        Self {
            dark: is_dark(base),
            base,
            text,
            text_dim: t.text_dim,
            text_faint: t.text_faint,
            link: accent_on_base(t.tertiary),
            accent: t.primary,
            accent_soft: t.primary_soft,
            on_accent: t.on_primary,
            user_fill,
            user_stroke: mix(user_fill, t.tertiary, USER_STROKE_MIX),
            user_text: legible(t.text, user_fill, base, MIN_TEXT_CONTRAST),
            card_fill: t.raised,
            card_stroke: t.card_border,
            card_text: legible(t.text, t.raised, base, MIN_TEXT_CONTRAST),
            popup_fill: t.window,
            sunken: t.sunken,
            code_plate: t.sunken.gamma_multiply(PLATE_ALPHA),
            code_chip: t.primary.gamma_multiply(CODE_CHIP_ALPHA),
            separator: t.separator,
            chip_fill,
            chip_stroke: t.field_border,
            chip_text: legible(t.text_dim, chip_fill, base, MIN_TEXT_CONTRAST),
            mention_fill,
            mention_text: legible(t.primary, mention_fill, base, MIN_TEXT_CONTRAST),
            running: accent_on_base(t.primary),
            success: accent_on_base(t.success),
            warn: accent_on_base(t.warn),
            error: accent_on_base(t.error),
            info: accent_on_base(t.info),
            json: JsonColors {
                key: accent_on_base(t.info),
                string: accent_on_base(t.success),
                number: accent_on_base(t.warn),
                boolean: accent_on_base(t.secondary),
                null: accent_on_base(t.secondary),
                index: t.text_dim,
                punct: t.text_dim,
                highlight: t.primary.gamma_multiply(HIGHLIGHT_ALPHA),
            },
            shell: ShellColors {
                command: accent_on_base(t.primary),
                flag: accent_on_base(t.info),
                string: accent_on_base(t.success),
                number: accent_on_base(t.warn),
                operator: accent_on_base(t.secondary),
                base: text,
            },
            radius_bubble: t.radius_lg.max(t.radius_md),
            radius_card: t.radius_md,
            radius_chip: t.radius_sm,
        }
    }

    /// The palette for the tokens published on `ctx`, rebuilt only when the tokens or the builder change.
    pub fn get(ctx: &Context) -> Self {
        let tokens = Tokens::get(ctx);
        let (generation, builder) = ctx
            .data(|d| d.get_temp::<(u64, PaletteBuilder)>(builder_id()))
            .unwrap_or((0, Self::from_tokens as PaletteBuilder));
        if let Some(c) = ctx.data(|d| d.get_temp::<Cached>(cache_id()))
            && c.tokens == tokens
            && c.generation == generation
        {
            return c.palette;
        }
        let palette = builder(&tokens);
        ctx.data_mut(|d| {
            d.insert_temp(
                cache_id(),
                Cached {
                    tokens,
                    generation,
                    palette,
                },
            );
        });
        palette
    }

    /// [`Self::get`] for the context `ui` paints into.
    pub fn of(ui: &Ui) -> Self {
        Self::get(ui.ctx())
    }

    /// Replaces the derivation [`Self::get`] runs on every token change.
    pub fn set_builder(ctx: &Context, builder: PaletteBuilder) {
        ctx.data_mut(|d| {
            let generation = d
                .get_temp::<(u64, PaletteBuilder)>(builder_id())
                .map_or(0, |(g, _)| g);
            d.insert_temp(builder_id(), (generation + 1, builder));
        });
    }

    /// Restores [`Self::from_tokens`] as the derivation.
    pub fn reset_builder(ctx: &Context) {
        Self::set_builder(ctx, Self::from_tokens);
    }
}

/// `fg`, moved toward the better ink until it reaches `min` contrast on `fill` composited over `base`.
pub fn legible(fg: Color32, fill: Color32, base: Color32, min: f32) -> Color32 {
    let bg = composite(fill, base);
    let fg = composite(fg, bg);
    if contrast(fg, bg) >= min {
        return fg;
    }
    let ink = ink_on(bg);
    let mut t = LEGIBLE_STEP;
    while t < 1.0 {
        let c = mix(fg, ink, t);
        if contrast(c, bg) >= min {
            return c;
        }
        t += LEGIBLE_STEP;
    }
    ink
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets::{MATCHABLE_PRESETS, apply_preset};
    use egui::{Style, Visuals};

    fn egui_default_tokens() -> [Tokens; 2] {
        let light = Style {
            visuals: Visuals::light(),
            ..Style::default()
        };
        let ctx = Context::default();
        let (success, secondary) = (
            crate::colors::success_ctx(&ctx),
            crate::colors::accent_secondary_ctx(&ctx),
        );
        [
            Tokens::derive(&Style::default(), success, secondary),
            Tokens::derive(&light, success, secondary),
        ]
    }

    fn all_tokens() -> Vec<(String, Tokens)> {
        let ctx = Context::default();
        let mut out: Vec<(String, Tokens)> = MATCHABLE_PRESETS
            .into_iter()
            .map(|preset| {
                apply_preset(&ctx, preset);
                (format!("{preset:?}"), Tokens::get(&ctx))
            })
            .collect();
        let [dark, light] = egui_default_tokens();
        out.push(("egui dark".into(), dark));
        out.push(("egui light".into(), light));
        out
    }

    #[test]
    fn text_reads_on_every_fill_under_every_preset() {
        for (name, t) in all_tokens() {
            let p = ChatPalette::from_tokens(&t);
            for (what, fg, fill) in [
                ("text on base", p.text, p.base),
                ("text on card", p.card_text, p.card_fill),
                ("user text", p.user_text, p.user_fill),
                ("mention text", p.mention_text, p.mention_fill),
                ("chip text", p.chip_text, p.chip_fill),
            ] {
                let bg = composite(fill, p.base);
                let ratio = contrast(composite(fg, bg), bg);
                assert!(ratio >= MIN_TEXT_CONTRAST, "{name}: {what} is {ratio:.2}:1");
            }
        }
    }

    #[test]
    fn dark_follows_the_panel() {
        let [dark, light] = egui_default_tokens();
        assert!(ChatPalette::from_tokens(&dark).dark);
        assert!(!ChatPalette::from_tokens(&light).dark);
    }

    #[test]
    fn the_cache_follows_token_and_builder_changes() {
        let ctx = Context::default();
        let first = ChatPalette::get(&ctx);
        assert_eq!(first, ChatPalette::from_tokens(&Tokens::get(&ctx)));
        let changed = Tokens {
            primary: Tokens::get(&ctx).error,
            ..Tokens::get(&ctx)
        };
        Tokens::publish(&ctx, changed);
        assert_eq!(ChatPalette::get(&ctx).accent, changed.primary);
        ChatPalette::set_builder(&ctx, |t| ChatPalette {
            radius_bubble: t.radius_sm,
            ..ChatPalette::from_tokens(t)
        });
        assert_eq!(ChatPalette::get(&ctx).radius_bubble, changed.radius_sm);
        ChatPalette::reset_builder(&ctx);
        assert_eq!(ChatPalette::get(&ctx), ChatPalette::from_tokens(&changed));
    }

    #[test]
    fn legible_keeps_readable_colours_and_lifts_faint_ones() {
        let [dark, _] = egui_default_tokens();
        assert_eq!(legible(dark.text, dark.base, dark.base, 4.5), dark.text);
        let lifted = legible(dark.base, dark.base, dark.base, 4.5);
        assert!(contrast(lifted, dark.base) >= 4.5);
    }
}
