//! Touch handling for the composer text: drag to scroll, hold to select, cursor tracking and the text action row.

use egui::{Context, Pos2, Rect};

/// Seconds a still press takes to become a hold.
pub(super) const HOLD_SECS: f64 = 0.35;
/// How far a press may land from the tap before it and still count as a double tap.
const DOUBLE_TAP_SLOP: f32 = 40.0;

/// A clipboard or selection action on the composer text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextAction {
    Cut,
    Copy,
    Paste,
    SelectAll,
}

impl TextAction {
    /// Every action in row order.
    pub const ALL: [Self; 4] = [Self::Cut, Self::Copy, Self::Paste, Self::SelectAll];

    pub fn label(self) -> &'static str {
        match self {
            Self::Cut => "Cut",
            Self::Copy => "Copy",
            Self::Paste => "Paste",
            Self::SelectAll => "Select all",
        }
    }
}

/// A press on the focused text.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) enum Gesture {
    #[default]
    Idle,
    /// Down, not yet held or moved; `before` is the selection the press replaced.
    Pressed {
        origin: Pos2,
        start: f64,
        before: Option<(usize, usize)>,
    },
    /// Moved before the hold: the text scrolls with the finger.
    Scrolling,
    /// Held, or pressed again right after a tap: egui and the host select text.
    Selecting,
}

/// Touch state kept between frames.
#[derive(Clone, Debug)]
pub(super) struct TouchState {
    pub gesture: Gesture,
    /// When and where the last tap on the text lifted.
    pub last_tap: Option<(f64, Pos2)>,
    /// Text viewport on screen.
    pub view: Rect,
    /// Cursor rect on screen.
    pub cursor: Rect,
    pub scroll_y: f32,
    pub scroll_max: f32,
    /// Height the text may grow to.
    pub max_h: f32,
    pub was_focused: bool,
    /// Focus comes back from the composer itself, so gaining it keeps the cursor.
    pub regain: bool,
    /// The cursor is scrolled into view on the next frame.
    pub track: bool,
    /// Hash of the text the text edit last drew.
    pub drawn: u64,
    pub pending_paste: Option<String>,
    /// Buttons of the text action row; empty while hidden.
    pub buttons: Vec<(TextAction, Rect)>,
    /// Width of the controls left of the text without the action row.
    pub base_left_w: f32,
}

impl Default for TouchState {
    fn default() -> Self {
        Self {
            gesture: Gesture::Idle,
            last_tap: None,
            view: Rect::NOTHING,
            cursor: Rect::NOTHING,
            scroll_y: 0.0,
            scroll_max: 0.0,
            max_h: 0.0,
            was_focused: false,
            regain: false,
            track: false,
            drawn: 0,
            pending_paste: None,
            buttons: Vec::new(),
            base_left_w: 0.0,
        }
    }
}

impl TouchState {
    /// Whether `pos` lies on a text action button.
    pub fn on_button(&self, pos: Pos2) -> bool {
        self.buttons.iter().any(|(_, r)| r.contains(pos))
    }
}

/// What a frame's gesture asks of the text.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Step {
    /// Points the finger moved the text down by.
    pub scroll_by: f32,
    /// Selection to put back, from before a press that turned into a scroll.
    pub restore: Option<(usize, usize)>,
}

/// Advances the gesture by this frame's input; a press counts when `focused` and inside the text view.
pub(super) fn advance(
    ctx: &Context,
    touch: &mut TouchState,
    focused: bool,
    cursor: Option<(usize, usize)>,
) -> Step {
    let TouchState {
        gesture,
        last_tap,
        view,
        ..
    } = touch;
    let (pressed, down, pos, delta, time) = ctx.input(|i| {
        (
            i.pointer
                .primary_pressed()
                .then(|| i.pointer.press_origin())
                .flatten(),
            i.pointer.primary_down(),
            i.pointer.latest_pos(),
            i.pointer.delta(),
            i.time,
        )
    });
    let (slop, double_delay) = ctx.options(|o| {
        (
            o.input_options.max_click_dist,
            o.input_options.max_double_click_delay,
        )
    });
    if let Some(origin) = pressed {
        let follows_tap = last_tap.take().is_some_and(|(t, p)| {
            time - t <= double_delay && p.distance(origin) <= DOUBLE_TAP_SLOP
        });
        *gesture = match (focused && view.contains(origin), follows_tap) {
            (false, _) => Gesture::Idle,
            (true, true) => Gesture::Selecting,
            (true, false) => Gesture::Pressed {
                origin,
                start: time,
                before: cursor,
            },
        };
    }
    let mut step = Step::default();
    let Some(pos) = pos.filter(|_| down) else {
        if let Gesture::Pressed { origin, .. } = *gesture {
            *last_tap = Some((time, origin));
        }
        *gesture = Gesture::Idle;
        return step;
    };
    match *gesture {
        Gesture::Pressed {
            origin,
            start,
            before,
        } => {
            let moved = origin.distance(pos) > slop;
            let held = time - start >= HOLD_SECS;
            if moved && !held {
                *gesture = Gesture::Scrolling;
                step.scroll_by = pos.y - origin.y;
                step.restore = before;
            } else if held {
                *gesture = Gesture::Selecting;
            } else {
                ctx.request_repaint_after_secs((start + HOLD_SECS - time) as f32);
            }
        }
        Gesture::Scrolling => step.scroll_by = delta.y,
        Gesture::Idle | Gesture::Selecting => {}
    }
    if *gesture == Gesture::Scrolling {
        ctx.stop_dragging();
    }
    step
}

pub(super) fn text_hash(text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}
