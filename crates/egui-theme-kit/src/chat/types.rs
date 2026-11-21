//! Shared chat types: borrowed text, ids, timestamps, tones, statuses and the actions widgets return.

use std::borrow::Cow;
use std::hash::Hash;

use egui::{AsIdSalt, Color32, Id};
use serde::{Deserialize, Serialize};

use super::mention::MentionRef;
use super::palette::ChatPalette;
use crate::colors::Status;

/// Text a widget borrows from the app or owns when the app formats it on the fly.
pub type CowStr<'a> = Cow<'a, str>;

/// A borrowed or owned list, covariant in its lifetime.
#[derive(Clone, Debug)]
pub enum CowList<'a, T> {
    Borrowed(&'a [T]),
    Owned(Vec<T>),
}

impl<T> Default for CowList<'_, T> {
    fn default() -> Self {
        Self::Borrowed(&[])
    }
}

impl<T> std::ops::Deref for CowList<'_, T> {
    type Target = [T];

    fn deref(&self) -> &[T] {
        match self {
            Self::Borrowed(items) => items,
            Self::Owned(items) => items,
        }
    }
}

impl<'a, T> From<&'a [T]> for CowList<'a, T> {
    fn from(items: &'a [T]) -> Self {
        Self::Borrowed(items)
    }
}

impl<T> From<Vec<T>> for CowList<'_, T> {
    fn from(items: Vec<T>) -> Self {
        Self::Owned(items)
    }
}

/// Id of `part` of the item keyed `key` under `parent`.
pub fn item_id(parent: Id, key: &str, part: impl AsIdSalt) -> Id {
    parent.with((key, part))
}

/// Milliseconds since the Unix epoch, UTC.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct UnixMs(pub i64);

impl UnixMs {
    pub const fn from_secs(secs: i64) -> Self {
        Self(secs.saturating_mul(1000))
    }

    pub const fn as_secs(self) -> i64 {
        self.0.div_euclid(1000)
    }

    /// Milliseconds from `earlier` to `self`, zero when `earlier` is later.
    pub fn since(self, earlier: Self) -> u64 {
        u64::try_from(self.0.saturating_sub(earlier.0)).unwrap_or(0)
    }

    /// The system clock; absent on targets without one.
    pub fn now() -> Option<Self> {
        #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
        {
            None
        }
        #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
        {
            let elapsed = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .ok()?;
            i64::try_from(elapsed.as_millis()).ok().map(Self)
        }
    }
}

/// Turns a timestamp into the short label chat rows show.
pub trait TimeFormat {
    /// Label for `t`; `now` lets the formatter drop the date for today.
    fn label(&self, t: UnixMs, now: Option<UnixMs>) -> String;
}

impl<F: Fn(UnixMs, Option<UnixMs>) -> String> TimeFormat for F {
    fn label(&self, t: UnixMs, now: Option<UnixMs>) -> String {
        self(t, now)
    }
}

/// Clock-time labels at a fixed UTC offset: `14:05` today, `Nov 20, 14:05` this year, `Nov 20, 2025` before.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClockFormat {
    /// Minutes added to UTC.
    pub utc_offset_min: i32,
    pub hour24: bool,
}

impl Default for ClockFormat {
    fn default() -> Self {
        Self {
            utc_offset_min: 0,
            hour24: true,
        }
    }
}

const MS_PER_DAY: i64 = 86_400_000;
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

impl ClockFormat {
    fn local(&self, t: UnixMs) -> i64 {
        t.0.saturating_add(i64::from(self.utc_offset_min) * 60_000)
    }

    fn clock(&self, ms_of_day: i64) -> String {
        let minutes = ms_of_day / 60_000;
        let (h, m) = (minutes / 60, minutes % 60);
        if self.hour24 {
            format!("{h:02}:{m:02}")
        } else {
            let suffix = if h < 12 { "AM" } else { "PM" };
            let h12 = if h % 12 == 0 { 12 } else { h % 12 };
            format!("{h12}:{m:02} {suffix}")
        }
    }
}

impl TimeFormat for ClockFormat {
    fn label(&self, t: UnixMs, now: Option<UnixMs>) -> String {
        let local = self.local(t);
        let (day, ms_of_day) = (local.div_euclid(MS_PER_DAY), local.rem_euclid(MS_PER_DAY));
        let (year, month, date) = civil_from_days(day);
        let month = MONTHS[(month - 1) as usize];
        let today = now.map(|n| self.local(n).div_euclid(MS_PER_DAY));
        let this_year = today.map(|d| civil_from_days(d).0);
        if today == Some(day) {
            self.clock(ms_of_day)
        } else if this_year == Some(year) {
            format!("{month} {date}, {}", self.clock(ms_of_day))
        } else {
            format!("{month} {date}, {year}")
        }
    }
}

/// Proleptic Gregorian `(year, month, day)` of a day count from 1970-01-01.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// `42 s`, `3 m 5 s` or `1 h 2 m`; under a second reads `<1 s`.
pub fn format_duration(ms: u64) -> String {
    let secs = ms / 1000;
    match secs {
        0 => "<1 s".to_owned(),
        1..60 => format!("{secs} s"),
        60..3600 => match secs % 60 {
            0 => format!("{} m", secs / 60),
            s => format!("{} m {s} s", secs / 60),
        },
        _ => match (secs % 3600) / 60 {
            0 => format!("{} h", secs / 3600),
            m => format!("{} h {m} m", secs / 3600),
        },
    }
}

/// Colour role of a chip, notice, badge or status.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Tone {
    #[default]
    Neutral,
    Accent,
    Info,
    Success,
    Warn,
    Error,
}

impl Tone {
    /// The palette colour of this tone; neutral is the dim text colour.
    pub fn color(self, p: &ChatPalette) -> Color32 {
        match self {
            Self::Neutral => p.text_dim,
            Self::Accent => p.accent,
            Self::Info => p.info,
            Self::Success => p.success,
            Self::Warn => p.warn,
            Self::Error => p.error,
        }
    }
}

impl From<Status> for Tone {
    fn from(status: Status) -> Self {
        match status {
            Status::Good => Self::Success,
            Status::Warn => Self::Warn,
            Status::Serious | Status::Critical => Self::Error,
        }
    }
}

/// Lifecycle state of an agent session.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SessionStatus {
    #[default]
    Idle,
    Working,
    /// Blocked on an approval or a question.
    Waiting,
    Error,
    Closed,
    Offline,
}

impl SessionStatus {
    pub fn tone(self) -> Tone {
        match self {
            Self::Idle | Self::Closed | Self::Offline => Tone::Neutral,
            Self::Working => Tone::Accent,
            Self::Waiting => Tone::Warn,
            Self::Error => Tone::Error,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Working => "Working",
            Self::Waiting => "Waiting",
            Self::Error => "Error",
            Self::Closed => "Closed",
            Self::Offline => "Offline",
        }
    }
}

/// Position among the user's own messages, as `current` of `total`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct MessageNav {
    /// Zero-based index of the message in view, if any.
    pub current: Option<usize>,
    pub total: usize,
}

/// A step through the user's own messages.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NavStep {
    Prev,
    Next,
    Pick(usize),
}

/// One answered question of a question card.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Answer {
    /// Id of the question.
    pub question: String,
    /// Ids of the picked items.
    pub picked: Vec<String>,
    /// Free text typed with the answer.
    pub note: Option<String>,
}

/// What the user did on an approval or question card.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum CardAction {
    /// Index of the picked approval choice.
    Choose(usize),
    Answer(Vec<Answer>),
    Dismiss,
}

/// What the user did in a transcript; the app carries it out.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ChatAction {
    /// Text placed on the clipboard.
    Copied(String),
    OpenMention(MentionRef),
    /// A link the user clicked.
    Link(String),
    Card {
        key: String,
        action: CardAction,
    },
    /// Key of a failed message to send again.
    Resend(String),
    /// Key of a message to load back into the composer.
    EditResend(String),
    /// Request for the history before the first entry.
    LoadOlder,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_dates_match_known_days() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
        assert_eq!(civil_from_days(20_412), (2025, 11, 20));
    }

    #[test]
    fn clock_labels_drop_the_date_for_today_and_the_time_for_other_years() {
        let fmt = ClockFormat::default();
        let t = UnixMs(1_763_647_500_000);
        assert_eq!(fmt.label(t, Some(t)), "14:05");
        let next_week = UnixMs(t.0 + 7 * MS_PER_DAY);
        assert_eq!(fmt.label(t, Some(next_week)), "Nov 20, 14:05");
        let next_year = UnixMs(t.0 + 400 * MS_PER_DAY);
        assert_eq!(fmt.label(t, Some(next_year)), "Nov 20, 2025");
        let twelve = ClockFormat {
            hour24: false,
            ..fmt
        };
        assert_eq!(twelve.label(t, Some(t)), "2:05 PM");
        let shifted = ClockFormat {
            utc_offset_min: -7 * 60,
            ..fmt
        };
        assert_eq!(shifted.label(t, Some(t)), "07:05");
    }

    #[test]
    fn durations_use_the_two_largest_units() {
        assert_eq!(format_duration(400), "<1 s");
        assert_eq!(format_duration(42_000), "42 s");
        assert_eq!(format_duration(185_000), "3 m 5 s");
        assert_eq!(format_duration(120_000), "2 m");
        assert_eq!(format_duration(3_720_000), "1 h 2 m");
        assert_eq!(UnixMs(5).since(UnixMs(9)), 0);
    }
}
