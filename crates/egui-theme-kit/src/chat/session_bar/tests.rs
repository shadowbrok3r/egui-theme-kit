use std::borrow::Cow;

use egui::{
    CentralPanel, Context, Event, FontDefinitions, Id, IdSalt, Key, Modifiers, PointerButton, Pos2,
    Rangef, RawInput, Rect, Shape, Style, Ui, vec2,
};

use super::layout::{badge_total, fit_count, nav_steps, nav_text, title_span};
use super::sessions::{row_id, row_matches};
use super::title_card::{owner_of, start_rename};
use super::*;
use crate::chat::types::{ClockFormat, CowStr, MessageNav, NavStep, SessionStatus, Tone, UnixMs};
use crate::controls::icons;
use crate::fonts;
use crate::metrics::{self, Density};

fn context() -> Context {
    let ctx = Context::default();
    let mut definitions = FontDefinitions::default();
    fonts::install(&mut definitions);
    ctx.set_fonts(definitions);
    metrics::set_density(&ctx, Density::Regular);
    metrics::install_overlaid_style(&ctx, Style::default());
    ctx
}

fn input(width: f32, events: Vec<Event>) -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 600.0))),
        events,
        ..Default::default()
    }
}

/// Runs one frame `width` wide and returns every painted text with its rect.
fn frame(
    ctx: &Context,
    width: f32,
    events: Vec<Event>,
    mut f: impl FnMut(&mut Ui),
) -> Vec<(String, Rect)> {
    let mut full = ctx.run_ui(input(width, events), |ui| {
        CentralPanel::default().show(ui, |ui| f(ui));
    });
    full.textures_delta.clear();
    full.shapes
        .iter()
        .filter_map(|s| match &s.shape {
            Shape::Text(t) => Some((t.galley.text().to_owned(), t.visual_bounding_rect())),
            _ => None,
        })
        .collect()
}

fn press(p: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos: p,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

fn key(key: Key) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

/// Clicks at `p` over two frames `width` wide.
fn click(ctx: &Context, width: f32, p: Pos2, mut f: impl FnMut(&mut Ui)) {
    frame(
        ctx,
        width,
        vec![Event::PointerMoved(p), press(p, true)],
        &mut f,
    );
    frame(ctx, width, vec![press(p, false)], &mut f);
}

fn has(texts: &[(String, Rect)], text: &str) -> bool {
    texts.iter().any(|(t, _)| t == text)
}

fn rect_of(texts: &[(String, Rect)], text: &str) -> Rect {
    texts
        .iter()
        .find(|(t, _)| t == text)
        .map(|(_, r)| *r)
        .unwrap_or_else(|| panic!("{text:?} was not painted"))
}

fn bar_id(ui: &Ui) -> Id {
    ui.make_persistent_id(IdSalt::new("bar"))
}

fn list_id(ui: &Ui) -> Id {
    ui.make_persistent_id(IdSalt::new("list"))
}

static MODELS: [CowStr<'static>; 2] = [Cow::Borrowed("gpt-5"), Cow::Borrowed("o3")];

const TITLE: &str = "Fix the scroll jump";
const SUBTITLE: &str = "desk \u{b7} ~/src/engine";

fn chips() -> Vec<OptionChip<'static, &'static str>> {
    vec![
        OptionChip::new("model", "Model", "gpt-5").kind(ChipKind::Menu {
            options: &MODELS,
            selected: Some(0),
        }),
        OptionChip::new("context", "Context", "42%").kind(ChipKind::Meter {
            fraction: 0.42,
            action: Some("Compact".into()),
        }),
        OptionChip::new("auto", "Auto", "On").kind(ChipKind::Toggle { on: true }),
        OptionChip::new("host", "Host", "DESK-01").tone(Tone::Info),
    ]
}

fn buttons() -> Vec<BarButton<'static, &'static str>> {
    vec![BarButton::new("files", icons::FILE, "Files").badge(2)]
}

/// Draws the sample title card and returns its action.
fn sample_bar(
    ui: &mut Ui,
    chips: &[OptionChip<'static, &'static str>],
    buttons: &[BarButton<'static, &'static str>],
    nav: MessageNav,
) -> Option<SessionBarAction<&'static str>> {
    SessionBar::new("bar", TITLE)
        .status(SessionStatus::Working, None)
        .subtitle(SUBTITLE)
        .renamable(true)
        .chips(chips)
        .buttons(buttons)
        .nav(nav)
        .show(ui)
}

const NAV: MessageNav = MessageNav::new(Some(1), 3);

#[test]
fn layouts_follow_the_width_in_control_heights() {
    assert_eq!(BarLayout::for_width(800.0, 24.0), BarLayout::Full);
    assert_eq!(BarLayout::for_width(700.0, 24.0), BarLayout::Elided);
    assert_eq!(BarLayout::for_width(500.0, 24.0), BarLayout::Folded);
    assert_eq!(BarLayout::for_width(380.0, 24.0), BarLayout::Compact);
    assert_eq!(BarLayout::for_width(800.0, 40.0), BarLayout::Folded);
    assert!(BarLayout::Folded.folds() && BarLayout::Compact.folds());
    assert!(!BarLayout::Elided.folds() && BarLayout::Compact.compact());
}

#[test]
fn the_title_centres_on_the_row_until_the_groups_crowd_it() {
    let row = Rangef::new(0.0, 600.0);
    let centred = title_span(row, 80.0, 200.0, 8.0, 100.0);
    assert_eq!((centred.min, centred.max), (208.0, 392.0));
    let between = title_span(row, 80.0, 300.0, 8.0, 100.0);
    assert_eq!((between.min, between.max), (88.0, 292.0));
    let squeezed = title_span(Rangef::new(0.0, 100.0), 80.0, 80.0, 8.0, 100.0);
    assert_eq!(squeezed.span(), 0.0);
}

#[test]
fn chips_that_do_not_fit_leave_room_for_the_more_button() {
    assert_eq!(fit_count(&[50.0, 50.0, 50.0], 10.0, 170.0, 24.0), 3);
    assert_eq!(fit_count(&[50.0, 50.0, 50.0], 10.0, 160.0, 24.0), 2);
    assert_eq!(fit_count(&[50.0, 50.0, 50.0], 10.0, 100.0, 24.0), 1);
    assert_eq!(fit_count(&[50.0], 10.0, 20.0, 24.0), 0);
    assert_eq!(fit_count(&[], 10.0, 0.0, 24.0), 0);
}

#[test]
fn the_counter_reads_one_based_and_the_ends_stop_the_steps() {
    let at = MessageNav::new;
    assert_eq!(nav_text(at(Some(1), 3)), "2/3");
    assert_eq!(nav_text(at(None, 3)), "\u{2013}/3");
    assert_eq!(nav_steps(at(Some(0), 3)), (false, true));
    assert_eq!(nav_steps(at(Some(2), 3)), (true, false));
    assert_eq!(nav_steps(at(None, 3)), (true, true));
    assert_eq!(nav_steps(at(None, 0)), (false, false));
}

#[test]
fn the_view_badge_sums_counts_in_the_most_urgent_tone() {
    let items = [
        ViewItem::new(0, "Chats").badge(2, Tone::Warn),
        ViewItem::new(1, "Inbox").badge(3, Tone::Error),
        ViewItem::new(2, "Board").badge(0, Tone::Error),
        ViewItem::new(3, "Logs"),
    ];
    assert_eq!(badge_total(&items), Some((5, Tone::Error)));
    assert_eq!(badge_total(&items[2..]), None);
}

#[test]
fn search_matches_the_title_or_the_subtitle() {
    assert!(row_matches("", "Anything", ""));
    assert!(row_matches("scrl", "Fix the scroll jump", ""));
    assert!(row_matches("engine", "Fix", "desk ~/src/engine"));
    assert!(!row_matches("zzz", "Fix the scroll jump", "desk"));
}

#[test]
fn a_wide_card_shows_every_row_with_the_title_centred() {
    let ctx = context();
    let (chips, buttons) = (chips(), buttons());
    let mut texts = Vec::new();
    for _ in 0..2 {
        texts = frame(&ctx, 900.0, Vec::new(), |ui| {
            let _ = sample_bar(ui, &chips, &buttons, NAV);
        });
    }
    for text in [
        TITLE,
        SUBTITLE,
        "Working",
        "Your messages",
        "2/3",
        "gpt-5",
        "42%",
        "DESK-01",
    ] {
        assert!(has(&texts, text), "{text:?} missing");
    }
    let title = rect_of(&texts, TITLE);
    assert!((title.center().x - 450.0).abs() < 2.0, "{title:?}");
    assert!(rect_of(&texts, "gpt-5").top() > rect_of(&texts, SUBTITLE).top());
}

#[test]
fn a_folded_card_moves_chips_into_the_menu_and_a_compact_one_drops_labels() {
    let ctx = context();
    let (chips, buttons) = (chips(), buttons());
    let mut folded = Vec::new();
    for _ in 0..2 {
        folded = frame(&ctx, 500.0, Vec::new(), |ui| {
            let _ = sample_bar(ui, &chips, &buttons, NAV);
        });
    }
    assert!(has(&folded, "Your messages") && has(&folded, "Working"));
    assert!(!has(&folded, "gpt-5"));
    let mut compact = Vec::new();
    for _ in 0..2 {
        compact = frame(&ctx, 360.0, Vec::new(), |ui| {
            let _ = sample_bar(ui, &chips, &buttons, NAV);
        });
    }
    assert!(has(&compact, TITLE) && has(&compact, "2/3"));
    for text in ["Your messages", "Working", SUBTITLE, "gpt-5"] {
        assert!(!has(&compact, text), "{text:?} shown when compact");
    }
}

#[test]
fn tapping_a_compact_title_shows_the_subtitle() {
    let ctx = context();
    let mut title = Rect::NOTHING;
    for _ in 0..2 {
        let texts = frame(&ctx, 360.0, Vec::new(), |ui| {
            let _ = sample_bar(ui, &[], &[], NAV);
        });
        title = rect_of(&texts, TITLE);
    }
    click(&ctx, 360.0, title.center(), |ui| {
        let _ = sample_bar(ui, &[], &[], NAV);
    });
    let texts = frame(&ctx, 360.0, Vec::new(), |ui| {
        let _ = sample_bar(ui, &[], &[], NAV);
    });
    assert!(has(&texts, SUBTITLE));
}

/// Clicks the widget `part` of the sample card and returns the card's actions.
fn click_part(nav: MessageNav, part: impl Fn(Id) -> Id) -> Vec<SessionBarAction<&'static str>> {
    let ctx = context();
    let (chips, buttons) = (chips(), buttons());
    let mut target = None;
    for _ in 0..2 {
        frame(&ctx, 900.0, Vec::new(), |ui| {
            target = Some(part(bar_id(ui)));
            let _ = sample_bar(ui, &chips, &buttons, nav);
        });
    }
    let id = target.expect("the frame ran");
    let rect = ctx
        .read_response(id)
        .map(|r| r.rect)
        .expect("the part was drawn");
    let mut actions = Vec::new();
    click(&ctx, 900.0, rect.center(), |ui| {
        actions.extend(sample_bar(ui, &chips, &buttons, nav));
    });
    actions
}

#[test]
fn jump_buttons_report_steps_and_stop_at_the_ends() {
    assert_eq!(
        click_part(NAV, |id| id.with("nav_prev")),
        vec![SessionBarAction::Nav(NavStep::Prev)]
    );
    assert_eq!(
        click_part(NAV, |id| id.with("nav_next")),
        vec![SessionBarAction::Nav(NavStep::Next)]
    );
    let first = MessageNav::new(Some(0), 3);
    assert!(click_part(first, |id| id.with("nav_prev")).is_empty());
}

#[test]
fn steps_past_the_loaded_messages_stay_enabled_when_more_exist() {
    let at = MessageNav::new;
    assert_eq!(nav_steps(at(Some(0), 3).more(true, false)), (true, true));
    assert_eq!(nav_steps(at(Some(2), 3).more(false, true)), (true, true));
    assert_eq!(nav_steps(at(Some(0), 1).more(true, false)), (true, false));
    assert_eq!(nav_steps(at(None, 0).more(true, false)), (true, false));
    assert_eq!(nav_steps(at(None, 0).more(false, true)), (false, true));
    assert_eq!(nav_steps(at(None, 0).more(true, true)), (true, true));
    assert_eq!(nav_steps(MessageNav::default()), (false, false));
}

#[test]
fn stepping_past_the_ends_reports_the_step_for_the_app_to_seek() {
    let first = MessageNav::new(Some(0), 3).more(true, false);
    assert_eq!(
        click_part(first, |id| id.with("nav_prev")),
        vec![SessionBarAction::Nav(NavStep::Prev)]
    );
    let last = MessageNav::new(Some(2), 3).more(false, true);
    assert_eq!(
        click_part(last, |id| id.with("nav_next")),
        vec![SessionBarAction::Nav(NavStep::Next)]
    );
    let none = MessageNav::new(None, 0).more(true, true);
    assert_eq!(
        click_part(none, |id| id.with("nav_prev")),
        vec![SessionBarAction::Nav(NavStep::Prev)]
    );
    assert_eq!(
        click_part(none, |id| id.with("nav_next")),
        vec![SessionBarAction::Nav(NavStep::Next)]
    );
    assert!(click_part(MessageNav::new(None, 0), |id| id.with("nav_next")).is_empty());
}

#[test]
fn chips_and_buttons_report_their_ids() {
    assert_eq!(
        click_part(NAV, |id| id.with(("chip", 2))),
        vec![SessionBarAction::ChipToggle {
            id: "auto",
            on: false
        }]
    );
    assert_eq!(
        click_part(NAV, |id| id.with(("button", 0))),
        vec![SessionBarAction::Button("files")]
    );
    assert!(click_part(NAV, |id| id.with(("chip", 3))).is_empty());
}

#[test]
fn renaming_commits_on_enter_and_cancels_on_escape() {
    let run = |finish: Key| {
        let ctx = context();
        let mut actions = Vec::new();
        let mut draw = |ui: &mut Ui, start: bool| {
            if start {
                start_rename(ui, bar_id(ui), TITLE, owner_of(None, TITLE));
            }
            actions.extend(sample_bar(ui, &[], &[], NAV));
        };
        frame(&ctx, 900.0, Vec::new(), |ui| draw(ui, true));
        frame(&ctx, 900.0, Vec::new(), |ui| draw(ui, false));
        let typed = vec![Event::Text("Renamed".into())];
        frame(&ctx, 900.0, typed, |ui| draw(ui, false));
        frame(&ctx, 900.0, vec![key(finish)], |ui| draw(ui, false));
        let texts = frame(&ctx, 900.0, Vec::new(), |ui| draw(ui, false));
        (actions, has(&texts, TITLE))
    };
    let (committed, _) = run(Key::Enter);
    assert!(
        matches!(committed.as_slice(), [SessionBarAction::Rename(name)] if name.contains("Renamed")),
        "{committed:?}"
    );
    let (cancelled, title_back) = run(Key::Escape);
    assert!(cancelled.is_empty(), "{cancelled:?}");
    assert!(title_back);
}

#[test]
fn a_rename_draft_is_dropped_when_the_card_shows_another_session() {
    let ctx = context();
    let mut actions = Vec::new();
    let mut draw = |ui: &mut Ui, session: &str, start: bool| {
        if start {
            start_rename(ui, bar_id(ui), TITLE, egui::util::hash("a"));
        }
        let bar = SessionBar::<&str>::new("bar", TITLE)
            .renamable(true)
            .session(session);
        actions.extend(bar.show(ui));
    };
    frame(&ctx, 900.0, Vec::new(), |ui| draw(ui, "a", true));
    frame(&ctx, 900.0, Vec::new(), |ui| draw(ui, "a", false));
    let typed = vec![Event::Text("Disk check".into())];
    frame(&ctx, 900.0, typed, |ui| draw(ui, "a", false));
    let texts = frame(&ctx, 900.0, Vec::new(), |ui| draw(ui, "b", false));
    assert!(has(&texts, TITLE));
    frame(&ctx, 900.0, vec![key(Key::Enter)], |ui| {
        draw(ui, "b", false)
    });
    frame(&ctx, 900.0, Vec::new(), |ui| draw(ui, "b", false));
    assert!(actions.is_empty(), "{actions:?}");
}

#[test]
fn an_error_callout_dismisses() {
    let ctx = context();
    let draw = |ui: &mut Ui| {
        SessionBar::<&str>::new("bar", TITLE)
            .error("Relay offline")
            .show(ui)
    };
    let mut target = None;
    for _ in 0..2 {
        let texts = frame(&ctx, 900.0, Vec::new(), |ui| {
            target = Some(bar_id(ui).with("dismiss"));
            let _ = draw(ui);
        });
        assert!(has(&texts, "Relay offline"));
    }
    let rect = target
        .and_then(|id| ctx.read_response(id))
        .map(|r| r.rect)
        .expect("the dismiss button was drawn");
    let mut actions = Vec::new();
    click(&ctx, 900.0, rect.center(), |ui| actions.extend(draw(ui)));
    assert_eq!(actions, vec![SessionBarAction::DismissError]);
}

fn views() -> [ViewItem<'static, u8>; 3] {
    [
        ViewItem::new(0, "Chats")
            .icon(icons::LIST)
            .badge(2, Tone::Warn),
        ViewItem::new(1, "Inbox").badge(3, Tone::Error),
        ViewItem::new(2, "Board"),
    ]
}

/// Draws the sample app bar and returns its action.
fn sample_app_bar(
    ui: &mut Ui,
    views: &[ViewItem<'static, u8>],
    detail: &mut DetailLevel,
    thinking: &mut bool,
) -> Option<AppBarAction<u8, &'static str>> {
    ChatAppBar::new("app")
        .view(ViewMenu::new(0, views))
        .detail(detail)
        .thinking(thinking)
        .show(ui, |_| {})
}

#[test]
fn the_app_bar_reports_detail_and_thinking_changes() {
    let ctx = context();
    let views = views();
    let (mut detail, mut thinking) = (DetailLevel::Simple, true);
    let mut texts = Vec::new();
    for _ in 0..2 {
        texts = frame(&ctx, 900.0, Vec::new(), |ui| {
            let _ = sample_app_bar(ui, &views, &mut detail, &mut thinking);
        });
    }
    for text in ["Chats", "5", "Simple", "Detailed", "Thinking"] {
        assert!(has(&texts, text), "{text:?} missing");
    }
    let mut actions = Vec::new();
    click(&ctx, 900.0, rect_of(&texts, "Thinking").center(), |ui| {
        actions.extend(sample_app_bar(ui, &views, &mut detail, &mut thinking));
    });
    assert_eq!(actions, vec![AppBarAction::Thinking(false)]);
    assert!(!thinking);
    actions.clear();
    click(&ctx, 900.0, rect_of(&texts, "Detailed").center(), |ui| {
        actions.extend(sample_app_bar(ui, &views, &mut detail, &mut thinking));
    });
    assert_eq!(actions, vec![AppBarAction::Detail(DetailLevel::Detailed)]);
    assert_eq!(detail, DetailLevel::Detailed);
}

#[test]
fn a_narrow_app_bar_folds_its_switches_into_a_menu() {
    let ctx = context();
    let views = views();
    let (mut detail, mut thinking) = (DetailLevel::Simple, true);
    let mut texts = Vec::new();
    for _ in 0..2 {
        texts = frame(&ctx, 400.0, Vec::new(), |ui| {
            let _ = sample_app_bar(ui, &views, &mut detail, &mut thinking);
        });
    }
    assert!(has(&texts, "Chats"));
    for text in ["Simple", "Detailed", "Thinking"] {
        assert!(!has(&texts, text), "{text:?} shown when narrow");
    }
}

#[test]
fn a_session_row_shows_two_lines_and_its_marks() {
    let ctx = context();
    let mut height = 0.0;
    let mut texts = Vec::new();
    for _ in 0..2 {
        texts = frame(&ctx, 400.0, Vec::new(), |ui| {
            height = SessionItem::new(TITLE)
                .subtitle(SUBTITLE)
                .status(SessionStatus::Waiting)
                .updated("14:05")
                .unread(3)
                .badge("Inbox", Tone::Info)
                .show(ui)
                .rect
                .height();
        });
    }
    for text in [TITLE, SUBTITLE, "14:05", "3", "Inbox"] {
        assert!(has(&texts, text), "{text:?} missing");
    }
    assert!(rect_of(&texts, SUBTITLE).top() >= rect_of(&texts, TITLE).bottom() - 1.0);
    assert!(rect_of(&texts, "14:05").right() <= 400.0);
    assert!(height >= 30.0, "{height}");
}

fn list_rows() -> Vec<SessionRow<'static, u32>> {
    vec![
        SessionRow::new(1, TITLE)
            .subtitle(SUBTITLE)
            .status(SessionStatus::Working)
            .updated(UnixMs(1_763_647_500_000)),
        SessionRow::new(2, "Port the composer").unread(4),
        SessionRow::new(3, "Desk").kind(RowKind::Manager),
    ]
}

/// Draws the sample session list and returns its action.
fn sample_list(
    ui: &mut Ui,
    sections: &[SessionSection<'_, u32>],
    query: &mut String,
) -> Option<SessionListAction<u32, u8>> {
    let format = ClockFormat::default();
    SessionList::new("list", sections)
        .search(query)
        .time_format(&format, Some(UnixMs(1_763_647_500_000)))
        .show(ui)
}

#[test]
fn the_session_list_filters_by_search_and_opens_a_clicked_row() {
    let ctx = context();
    let rows = list_rows();
    let sections = [SessionSection {
        title: "Today".into(),
        rows: &rows,
        collapsed_by_default: false,
    }];
    let mut query = String::new();
    let mut texts = Vec::new();
    for _ in 0..2 {
        texts = frame(&ctx, 400.0, Vec::new(), |ui| {
            let _ = sample_list(ui, &sections, &mut query);
        });
    }
    assert!(texts.iter().any(|(t, _)| t.starts_with("Today")));
    for text in [TITLE, "Port the composer", "Desk", "Manager", "14:05", "4"] {
        assert!(has(&texts, text), "{text:?} missing");
    }
    let mut target = None;
    frame(&ctx, 400.0, Vec::new(), |ui| {
        target = Some(row_id(list_id(ui), &2_u32));
        let _ = sample_list(ui, &sections, &mut query);
    });
    let rect = target
        .and_then(|id| ctx.read_response(id))
        .map(|r| r.rect)
        .expect("the row was drawn");
    let mut actions = Vec::new();
    click(&ctx, 400.0, rect.center(), |ui| {
        actions.extend(sample_list(ui, &sections, &mut query));
    });
    assert_eq!(actions, vec![SessionListAction::Open(2)]);

    query = "compo".to_owned();
    let texts = frame(&ctx, 400.0, Vec::new(), |ui| {
        let _ = sample_list(ui, &sections, &mut query);
    });
    assert!(has(&texts, "Port the composer"));
    assert!(!has(&texts, TITLE) && !has(&texts, "Desk"));
}

#[test]
fn icon_only_lists_draw_no_text() {
    let ctx = context();
    let rows = list_rows();
    let sections = [SessionSection {
        title: "Today".into(),
        rows: &rows,
        collapsed_by_default: false,
    }];
    let mut texts = Vec::new();
    for _ in 0..2 {
        texts = frame(&ctx, 60.0, Vec::new(), |ui| {
            let _ = SessionList::<u32, u8>::new("list", &sections)
                .icon_only(true)
                .show(ui);
        });
    }
    for text in [TITLE, "Manager"] {
        assert!(!has(&texts, text), "{text:?} shown when icon-only");
    }
    assert!(!texts.iter().any(|(t, _)| t.starts_with("Today")));
    assert!(has(&texts, "4"), "the unread count stays on the dot");
}

#[test]
fn row_menus_hide_and_disable_entries_per_row() {
    let ctx = context();
    let rows = list_rows();
    let sections = [SessionSection {
        title: "Today".into(),
        rows: &rows,
        collapsed_by_default: false,
    }];
    let menu = [
        MenuEntry {
            id: 1_u8,
            label: "Close".into(),
            icon: None,
            danger: false,
        },
        MenuEntry {
            id: 2_u8,
            label: "Take over".into(),
            icon: None,
            danger: false,
        },
    ];
    let state = |key: &u32, item: &u8| match (key, item) {
        (1, 1) => MenuEntryState::Hidden,
        (1, 2) => MenuEntryState::Disabled,
        _ => MenuEntryState::Shown,
    };
    let actions = std::cell::RefCell::new(Vec::new());
    let target = std::cell::Cell::new(None);
    let draw = |ui: &mut Ui| {
        target.set(Some(row_id(list_id(ui), &1_u32)));
        actions.borrow_mut().extend(
            SessionList::new("list", &sections)
                .row_menu(&menu)
                .row_menu_state(&state)
                .show(ui),
        );
    };
    frame(&ctx, 400.0, Vec::new(), draw);
    let rect = target
        .get()
        .and_then(|id| ctx.read_response(id))
        .map(|r| r.rect)
        .expect("the row was drawn");
    let p = rect.center();
    let secondary = |pressed| Event::PointerButton {
        pos: p,
        button: PointerButton::Secondary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    frame(
        &ctx,
        400.0,
        vec![Event::PointerMoved(p), secondary(true)],
        draw,
    );
    frame(&ctx, 400.0, vec![secondary(false)], draw);
    let texts = frame(&ctx, 400.0, Vec::new(), draw);
    assert!(has(&texts, "Take over"), "{texts:?}");
    assert!(!has(&texts, "Close"));
    assert!(!has(&texts, "Mention in chat"));
    click(&ctx, 400.0, rect_of(&texts, "Take over").center(), draw);
    assert!(actions.borrow().is_empty(), "{:?}", actions.borrow());
}

/// Runs one frame `width` wide and returns every painted shape.
fn frame_shapes(
    ctx: &Context,
    width: f32,
    events: Vec<Event>,
    mut f: impl FnMut(&mut Ui),
) -> Vec<Shape> {
    let mut full = ctx.run_ui(input(width, events), |ui| {
        CentralPanel::default().show(ui, |ui| f(ui));
    });
    full.textures_delta.clear();
    full.shapes.into_iter().map(|s| s.shape).collect()
}

fn shape_texts(shapes: &[Shape]) -> Vec<(String, Rect)> {
    shapes
        .iter()
        .filter_map(|s| match s {
            Shape::Text(t) => Some((t.galley.text().to_owned(), t.visual_bounding_rect())),
            _ => None,
        })
        .collect()
}

type Draw<'d> = dyn FnMut(&mut Ui) -> Option<SessionBarAction<&'static str>> + 'd;

/// Clicks the widget `part` of the card `draw` paints `width` wide; returns the card's actions.
fn click_in(
    width: f32,
    draw: &mut Draw<'_>,
    part: impl Fn(Id) -> Id,
) -> Vec<SessionBarAction<&'static str>> {
    let ctx = context();
    let mut target = None;
    for _ in 0..2 {
        frame(&ctx, width, Vec::new(), |ui| {
            target = Some(part(bar_id(ui)));
            let _ = draw(ui);
        });
    }
    let rect = target
        .and_then(|id| ctx.read_response(id))
        .map(|r| r.rect)
        .expect("the part was drawn");
    let mut actions = Vec::new();
    click(&ctx, width, rect.center(), |ui| actions.extend(draw(ui)));
    actions
}

/// Opens the menu of the card's `part`, clicks the row reading `row`; returns the open menu's shapes and the actions.
fn pick_in_menu(
    width: f32,
    draw: &mut Draw<'_>,
    part: impl Fn(Id) -> Id,
    row: &str,
) -> (Vec<Shape>, Vec<SessionBarAction<&'static str>>) {
    let ctx = context();
    let mut target = None;
    for _ in 0..2 {
        frame(&ctx, width, Vec::new(), |ui| {
            target = Some(part(bar_id(ui)));
            let _ = draw(ui);
        });
    }
    let rect = target
        .and_then(|id| ctx.read_response(id))
        .map(|r| r.rect)
        .expect("the menu button was drawn");
    click(&ctx, width, rect.center(), |ui| {
        let _ = draw(ui);
    });
    let mut shapes = Vec::new();
    for _ in 0..2 {
        shapes = frame_shapes(&ctx, width, Vec::new(), |ui| {
            let _ = draw(ui);
        });
    }
    let texts = shape_texts(&shapes);
    let mut actions = Vec::new();
    click(&ctx, width, rect_of(&texts, row).center(), |ui| {
        actions.extend(draw(ui));
    });
    (shapes, actions)
}

#[test]
fn a_disabled_button_ignores_clicks_inline_and_in_the_overflow_menu() {
    let off = [BarButton::new("files", icons::FILE, "Files").enabled(false)];
    let on = [BarButton::new("files", icons::FILE, "Files")];
    assert!(on[0].enabled);
    let draw = |buttons: &[BarButton<'static, &'static str>]| {
        let buttons = buttons.to_vec();
        move |ui: &mut Ui| sample_bar(ui, &[], &buttons, NAV)
    };
    assert!(click_in(900.0, &mut draw(&off), |id| id.with(("button", 0))).is_empty());
    assert_eq!(
        click_in(900.0, &mut draw(&on), |id| id.with(("button", 0))),
        vec![SessionBarAction::Button("files")]
    );
    let (_, picked) = pick_in_menu(500.0, &mut draw(&off), |id| id.with("more"), "Files");
    assert!(picked.is_empty(), "{picked:?}");
    let (_, picked) = pick_in_menu(500.0, &mut draw(&on), |id| id.with("more"), "Files");
    assert_eq!(picked, vec![SessionBarAction::Button("files")]);
}

fn meter_chip(enabled: bool) -> [OptionChip<'static, &'static str>; 1] {
    [OptionChip::new("context", "Context", "42%")
        .kind(ChipKind::Meter {
            fraction: 0.42,
            action: Some("Compact".into()),
        })
        .action_enabled(enabled)]
}

#[test]
fn a_greyed_meter_action_shows_but_does_not_fire() {
    for (enabled, folded) in [(false, false), (true, false), (false, true), (true, true)] {
        let chips = meter_chip(enabled);
        let mut draw = |ui: &mut Ui| sample_bar(ui, &chips, &[], NAV);
        let (width, part): (f32, fn(Id) -> Id) = if folded {
            (500.0, |id| id.with("more"))
        } else {
            (900.0, |id| id.with(("chip", 0)))
        };
        let (shapes, picked) = pick_in_menu(width, &mut draw, part, "Compact");
        assert!(has(&shape_texts(&shapes), "Compact"));
        let expected = if enabled {
            vec![SessionBarAction::ChipAction("context")]
        } else {
            Vec::new()
        };
        assert_eq!(picked, expected, "enabled {enabled}, folded {folded}");
    }
}

#[test]
fn a_folded_meter_row_draws_its_fill() {
    let chips = meter_chip(true);
    let mut draw = |ui: &mut Ui| sample_bar(ui, &chips, &[], NAV);
    let (shapes, _) = pick_in_menu(500.0, &mut draw, |id| id.with("more"), "Compact");
    let bars: Vec<Rect> = shapes
        .iter()
        .filter_map(|s| match s {
            Shape::Rect(r) if (r.rect.height() - 4.0).abs() < 0.01 => Some(r.rect),
            _ => None,
        })
        .collect();
    let track = super::chips::MENU_METER_W;
    let near = |w: f32| bars.iter().find(|r| (r.width() - w).abs() < 0.5);
    let (track_rect, fill) = (near(track), near(track * 0.42));
    assert!(track_rect.is_some() && fill.is_some(), "{bars:?}");
    let (track_rect, fill) = (track_rect.unwrap(), fill.unwrap());
    assert_eq!(fill.left(), track_rect.left());
    let caption = rect_of(&shape_texts(&shapes), "Context: 42%");
    assert!((fill.center().y - caption.center().y).abs() < 4.0);
    assert!(fill.left() > caption.right());
}

/// Draws a card with `status_in_subtitle` set to `flag` and returns its painted texts after a tap on the title.
fn tapped_compact(
    flag: Option<bool>,
    subtitle: Option<&'static str>,
    touch: bool,
) -> Vec<(String, Rect)> {
    let ctx = context();
    let draw = |ui: &mut Ui| {
        let mut bar = SessionBar::<&str>::new("bar", TITLE).status(SessionStatus::Working, None);
        if let Some(subtitle) = subtitle {
            bar = bar.subtitle(subtitle);
        }
        if let Some(on) = flag {
            bar = bar.status_in_subtitle(on);
        }
        let _ = bar.show(ui);
    };
    let finger = Event::Touch {
        device_id: egui::TouchDeviceId(1),
        id: egui::TouchId(1),
        phase: egui::TouchPhase::Cancel,
        pos: Pos2::ZERO,
        force: None,
    };
    let first = if touch { vec![finger] } else { Vec::new() };
    let mut title = Rect::NOTHING;
    for events in [first, Vec::new()] {
        let texts = frame(&ctx, 360.0, events, draw);
        assert!(
            !has(&texts, "Working"),
            "the status label shows before a tap"
        );
        title = rect_of(&texts, TITLE);
    }
    click(&ctx, 360.0, title.center(), draw);
    frame(&ctx, 360.0, Vec::new(), draw)
}

#[test]
fn a_compact_card_can_lead_the_subtitle_with_the_status_label() {
    let line = format!("Working \u{b7} {SUBTITLE}");
    assert!(has(
        &tapped_compact(Some(true), Some(SUBTITLE), false),
        &line
    ));
    assert!(has(&tapped_compact(Some(true), None, false), "Working"));
    assert!(has(&tapped_compact(None, Some(SUBTITLE), true), &line));
    let plain = tapped_compact(None, Some(SUBTITLE), false);
    assert!(has(&plain, SUBTITLE) && !has(&plain, &line));
    let off = tapped_compact(Some(false), Some(SUBTITLE), true);
    assert!(has(&off, SUBTITLE) && !has(&off, &line));

    let ctx = context();
    let mut wide = Vec::new();
    for _ in 0..2 {
        wide = frame(&ctx, 900.0, Vec::new(), |ui| {
            let _ = SessionBar::<&str>::new("bar", TITLE)
                .status(SessionStatus::Working, None)
                .subtitle(SUBTITLE)
                .status_in_subtitle(true)
                .show(ui);
        });
    }
    assert!(has(&wide, "Working") && has(&wide, SUBTITLE) && !has(&wide, &line));
}

#[test]
fn the_more_button_paints_three_dots() {
    let ctx = context();
    let (chips, buttons) = (chips(), buttons());
    let mut target = None;
    let mut shapes = Vec::new();
    for _ in 0..2 {
        shapes = frame_shapes(&ctx, 500.0, Vec::new(), |ui| {
            target = Some(bar_id(ui).with("more"));
            let _ = sample_bar(ui, &chips, &buttons, NAV);
        });
    }
    let rect = target
        .and_then(|id| ctx.read_response(id))
        .map(|r| r.rect)
        .expect("the more button was drawn");
    let dots = shapes
        .iter()
        .filter(|s| matches!(s, Shape::Circle(c) if rect.contains(c.center)))
        .count();
    assert_eq!(dots, 3);
}
