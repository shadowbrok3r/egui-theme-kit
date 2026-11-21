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
    /// Down, not yet held or moved; `before` is the selection kept until the press resolves.
    Pressed {
        origin: Pos2,
        start: f64,
        before: Option<(usize, usize)>,
    },
    /// Moved before the hold: the text scrolls with the finger.
    Scrolling,
    /// Pressed again right after a tap: egui selects text.
    Selecting,
    /// Held still: the word at `origin` is selected and the finger extends the selection.
    Holding {
        origin: Pos2,
        word: Option<(usize, usize)>,
    },
}

/// Touch state kept between frames.
#[derive(Clone, Debug)]
pub(super) struct TouchState {
    pub gesture: Gesture,
    /// When and where the last tap on the text lifted.
    pub last_tap: Option<(f64, Pos2)>,
    /// The last primary press came from a finger.
    pub finger: bool,
    /// Finger speed down the screen, in points per second, while scrolling or coasting.
    pub velocity: f32,
    /// The text coasts after a scroll lifted.
    pub coasting: bool,
    /// Text viewport on screen.
    pub view: Rect,
    /// Cursor rect on screen.
    pub cursor: Rect,
    pub scroll_y: f32,
    pub scroll_max: f32,
    /// Height the text may grow to.
    pub max_h: f32,
    pub was_focused: bool,
    /// The cursor is scrolled into view on the next frame.
    pub track: bool,
    /// Hash of the text the text edit last drew.
    pub drawn: u64,
    pub pending_paste: Option<String>,
    /// Buttons of the text action row; empty while hidden.
    pub buttons: Vec<(TextAction, Rect)>,
    /// Width of the controls left of the text without the action row.
    pub base_left_w: f32,
    /// Width of the right controls that do not depend on the activity.
    pub fixed_right_w: f32,
}

impl Default for TouchState {
    fn default() -> Self {
        Self {
            gesture: Gesture::Idle,
            last_tap: None,
            finger: false,
            velocity: 0.0,
            coasting: false,
            view: Rect::NOTHING,
            cursor: Rect::NOTHING,
            scroll_y: 0.0,
            scroll_max: 0.0,
            max_h: 0.0,
            was_focused: false,
            track: false,
            drawn: 0,
            pending_paste: None,
            buttons: Vec::new(),
            base_left_w: 0.0,
            fixed_right_w: 0.0,
        }
    }
}

impl TouchState {
    /// Whether `pos` lies on a text action button.
    pub fn on_button(&self, pos: Pos2) -> bool {
        self.buttons.iter().any(|(_, r)| r.contains(pos))
    }

    /// A drag scroll or its coast is moving the text.
    pub fn scrolling(&self) -> bool {
        self.gesture == Gesture::Scrolling || self.coasting
    }
}

/// What a frame's gesture asks of the text.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Step {
    /// Points the finger or the coast moved the text down by.
    pub scroll_by: f32,
    /// Selection to put back after the text edit ran, while a press is undecided.
    pub keep: Option<(usize, usize)>,
    /// A tap lifted here: the cursor goes to this point.
    pub tap: Option<Pos2>,
    /// A hold began here: the word under it is selected.
    pub hold: Option<Pos2>,
    /// A held selection follows the finger to here.
    pub extend: Option<Pos2>,
}

/// Points per second squared a coast slows by.
const COAST_FRICTION: f32 = 1000.0;
/// Speed, in points per second, below which a coast stops.
const COAST_STOP: f32 = 20.0;

/// Advances the gesture by this frame's input; a press counts when `focused`, inside the text view and from a finger.
pub(super) fn advance(
    ctx: &Context,
    touch: &mut TouchState,
    focused: bool,
    forced: bool,
    cursor: Option<(usize, usize)>,
) -> Step {
    let (pressed, finger, down, pos, delta, time, dt) = ctx.input(|i| {
        (
            i.pointer
                .primary_pressed()
                .then(|| i.pointer.press_origin())
                .flatten(),
            forced || i.any_touches(),
            i.pointer.primary_down(),
            i.pointer.latest_pos(),
            i.pointer.delta(),
            i.time,
            i.stable_dt.max(f32::EPSILON),
        )
    });
    let (slop, double_delay) = ctx.options(|o| {
        (
            o.input_options.max_click_dist,
            o.input_options.max_double_click_delay,
        )
    });
    if let Some(origin) = pressed {
        touch.finger = finger;
        touch.coasting = false;
        touch.velocity = 0.0;
        let follows_tap = touch.last_tap.take().is_some_and(|(t, p)| {
            time - t <= double_delay && p.distance(origin) <= DOUBLE_TAP_SLOP
        });
        touch.gesture = match (
            finger && focused && touch.view.contains(origin),
            follows_tap,
        ) {
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
        match touch.gesture {
            Gesture::Pressed { origin, .. } => {
                touch.last_tap = Some((time, origin));
                step.tap = Some(origin);
            }
            Gesture::Scrolling => {
                touch.coasting = focused && touch.velocity.abs() >= COAST_STOP;
            }
            _ => {}
        }
        touch.gesture = Gesture::Idle;
        if touch.coasting {
            step.scroll_by = coast(touch, dt);
            ctx.request_repaint();
        }
        return step;
    };
    match touch.gesture {
        Gesture::Pressed {
            origin,
            start,
            before,
        } => {
            let moved = origin.distance(pos) > slop;
            let held = time - start >= HOLD_SECS;
            if moved && !held {
                touch.gesture = Gesture::Scrolling;
                step.scroll_by = pos.y - origin.y;
                touch.velocity = step.scroll_by / dt;
                step.keep = before;
            } else if held {
                touch.gesture = Gesture::Holding { origin, word: None };
                step.hold = Some(origin);
            } else {
                step.keep = before;
                ctx.request_repaint_after_secs((start + HOLD_SECS - time) as f32);
            }
        }
        Gesture::Scrolling => {
            step.scroll_by = delta.y;
            touch.velocity = (touch.velocity + delta.y / dt) / 2.0;
        }
        Gesture::Holding { .. } => step.extend = Some(pos),
        Gesture::Idle | Gesture::Selecting => {}
    }
    if touch.gesture == Gesture::Scrolling {
        ctx.stop_dragging();
    }
    step
}

/// One frame of a coast: returns the points it moves and slows the speed; stops at either end of the text.
fn coast(touch: &mut TouchState, dt: f32) -> f32 {
    let v = touch.velocity;
    let at_end =
        (v > 0.0 && touch.scroll_y <= 0.0) || (v < 0.0 && touch.scroll_y >= touch.scroll_max);
    let slowed = v.abs() - COAST_FRICTION * dt;
    if at_end || slowed < COAST_STOP {
        touch.coasting = false;
        touch.velocity = 0.0;
        return 0.0;
    }
    touch.velocity = slowed.copysign(v);
    v * dt
}

/// Chars of the word around `at` in `text`; an empty range off a word.
pub(super) fn word_at(text: &str, at: usize) -> (usize, usize) {
    let is_word = |c: &&char| c.is_alphanumeric() || **c == '_';
    let chars: Vec<char> = text.chars().collect();
    let at = at.min(chars.len());
    let start = at - chars[..at].iter().rev().take_while(is_word).count();
    let end = at + chars[at..].iter().take_while(is_word).count();
    (start, end)
}

/// Selection of a hold on `word` with the finger at `head`: the word stays selected and grows toward the finger.
pub(super) fn extend_word((start, end): (usize, usize), head: usize) -> (usize, usize) {
    if head < start {
        (end, head)
    } else if head > end {
        (start, head)
    } else {
        (start, end)
    }
}

pub(super) fn text_hash(text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}
