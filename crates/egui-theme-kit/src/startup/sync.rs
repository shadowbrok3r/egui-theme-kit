//! Store readiness and change polling: the theme of a late-opening store reaches the live context.

use eframe::egui::Context;

use super::KitState;
use crate::{
    persist::{self, ThemeSource},
    presets::{apply_saved_theme, themes_equal},
    saved::SavedTheme,
    store::ThemeStore,
};

/// What one [`poll_store`] call did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorePoll {
    /// No store is registered, or nothing changed since the last poll.
    Idle,
    /// The store is registered but not ready yet.
    Waiting,
    /// The store changed or became ready and the live theme stayed as it was.
    Kept,
    /// The store's theme was applied to the context.
    Applied,
    /// The empty store received the live theme.
    Seeded,
}

/// The kit's view of the store between polls.
#[derive(Clone, Default)]
pub(super) struct SyncState {
    /// Store revision handled by the last poll.
    pub seen_revision: Option<u64>,
    /// Whether the store's first ready state has been acted on.
    pub ready_handled: bool,
    /// The live theme when it last matched the store or the kit applied it; a different live theme has local edits.
    pub synced: Option<SavedTheme>,
    /// The store's active theme at the last poll.
    pub last_active: Option<SavedTheme>,
    /// Themes applied by polling.
    pub epoch: u64,
}

impl SyncState {
    /// State for a store registered while `live` is on screen; `applied` says the startup theme came from the store.
    ///
    /// A ready store without an active theme receives a custom `live` theme here, like a store that becomes ready later.
    pub(super) fn attached(
        ctx: &Context,
        store: &dyn ThemeStore,
        live: SavedTheme,
        applied: bool,
    ) -> Self {
        let mut state = Self {
            seen_revision: applied.then(|| store.revision()),
            ready_handled: applied,
            synced: Some(live.clone()),
            last_active: applied.then(|| store.load_active()).flatten(),
            epoch: 0,
        };
        if applied
            && state.last_active.is_none()
            && !from_file(ctx)
            && !persist::is_default_in(ctx, &live)
        {
            store.save_active(&live);
            state.seen_revision = Some(store.revision());
            state.last_active = store.load_active();
        }
        state
    }
}

/// Applies the registered store's theme to `ctx` when the store first becomes ready and when its active theme changes; call every frame.
///
/// [`init`](crate::init) registers a plugin that calls this at the start of every pass, so apps only
/// call it themselves to poll at another moment. It reads two counters and returns unless the store
/// became ready or its [`revision`](ThemeStore::revision) changed. A `--theme` file theme is never replaced.
pub fn poll_store(ctx: &Context) -> StorePoll {
    let Some((store, seen, handled)) = ctx
        .with_plugin::<KitState, _>(|state| {
            state
                .store
                .clone()
                .map(|store| (store, state.sync.seen_revision, state.sync.ready_handled))
        })
        .flatten()
    else {
        return StorePoll::Idle;
    };
    if !store.is_ready() {
        return StorePoll::Waiting;
    }
    let revision = store.revision();
    if handled && seen == Some(revision) {
        return StorePoll::Idle;
    }

    let mut sync = ctx
        .with_plugin::<KitState, _>(|state| state.sync.clone())
        .unwrap_or_default();
    let outcome = if handled {
        on_change(ctx, &*store, &mut sync)
    } else {
        on_ready(ctx, &*store, &mut sync)
    };
    sync.seen_revision = Some(revision);
    sync.ready_handled = true;
    ctx.with_plugin::<KitState, _>(|state| {
        if state
            .store
            .as_ref()
            .is_some_and(|current| std::sync::Arc::ptr_eq(current, &store))
        {
            state.sync = sync;
        }
    });
    if outcome == StorePoll::Applied {
        ctx.request_repaint();
        ctx.request_discard("egui_theme_kit: the stored theme was applied");
    }
    outcome
}

fn from_file(ctx: &Context) -> bool {
    matches!(persist::source(ctx), ThemeSource::File(_))
}

/// Whether two optional themes agree in every part.
fn same_active(a: &Option<SavedTheme>, b: &Option<SavedTheme>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => themes_equal(a, b),
        _ => false,
    }
}

/// Installs `theme` from the store and records it as in sync.
fn apply(ctx: &Context, sync: &mut SyncState, theme: SavedTheme) {
    apply_saved_theme(ctx, theme);
    persist::set_source(ctx, ThemeSource::Stored);
    sync.synced = Some(SavedTheme::current(ctx));
    sync.epoch += 1;
}

/// The store's first ready state: the pinned or active theme replaces an untouched live theme.
fn on_ready(ctx: &Context, store: &dyn ThemeStore, sync: &mut SyncState) -> StorePoll {
    let live = SavedTheme::current(ctx);
    let edited = sync
        .synced
        .as_ref()
        .is_some_and(|synced| !themes_equal(&live, synced));
    let outcome = if from_file(ctx) || edited {
        StorePoll::Kept
    } else if let Some(theme) = persist::store_theme(store) {
        if themes_equal(&live, &theme) {
            persist::set_source(ctx, ThemeSource::Stored);
            sync.synced = Some(live);
            StorePoll::Kept
        } else {
            apply(ctx, sync, theme);
            StorePoll::Applied
        }
    } else if !persist::is_default_in(ctx, &live) {
        store.save_active(&live);
        sync.synced = Some(live);
        StorePoll::Seeded
    } else {
        sync.synced = Some(live);
        StorePoll::Kept
    };
    sync.last_active = store.load_active();
    if sync
        .last_active
        .as_ref()
        .is_some_and(|active| themes_equal(active, &SavedTheme::current(ctx)))
    {
        sync.synced = Some(SavedTheme::current(ctx));
    }
    outcome
}

/// A later store change: a new active theme from elsewhere replaces a live theme without local edits.
fn on_change(ctx: &Context, store: &dyn ThemeStore, sync: &mut SyncState) -> StorePoll {
    let active = store.load_active();
    let changed = !same_active(&active, &sync.last_active);
    sync.last_active = active.clone();
    if !changed || from_file(ctx) {
        return StorePoll::Idle;
    }
    let live = SavedTheme::current(ctx);
    let Some(active) = active else {
        if persist::is_default_in(ctx, &live) {
            sync.synced = Some(live);
        }
        return StorePoll::Kept;
    };
    if themes_equal(&live, &active) {
        sync.synced = Some(live);
        return StorePoll::Kept;
    }
    let edited = sync
        .synced
        .as_ref()
        .is_some_and(|synced| !themes_equal(&live, synced));
    if edited {
        return StorePoll::Kept;
    }
    apply(ctx, sync, active);
    StorePoll::Applied
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use eframe::egui::{Context, RawInput};

    use super::*;
    use crate::{
        metrics::Density,
        presets::{self, apply_preset, installed_preset},
        startup::{
            KitConfig, adopt_live_theme, attach_store, init_context,
            tests::{others, preset_of, preset_theme},
            theme_epoch,
        },
        store::{
            AutoSave, CachedStore, MemoryStore, StartupTheme,
            testing::{SharedBackend, connect},
        },
    };

    /// A context started over a lazily opened store, and the backend rows behind it.
    struct Rig {
        ctx: Context,
        store: Arc<CachedStore>,
        backend: SharedBackend,
    }

    impl Rig {
        fn new() -> Self {
            Self::with_config(|cfg| cfg)
        }

        fn with_config(configure: impl FnOnce(KitConfig) -> KitConfig) -> Self {
            let backend = SharedBackend::default();
            let store = Arc::new(CachedStore::open_lazy(backend.clone()));
            let ctx = Context::default();
            init_context(
                &ctx,
                None,
                configure(KitConfig::new().parse_cli(false).store(store.clone())),
            );
            Self {
                ctx,
                store,
                backend,
            }
        }

        /// Puts `theme` in the backend as the stored active theme.
        fn stored(&self, theme: SavedTheme) {
            self.backend.rows.lock().unwrap().active = Some(theme);
        }

        fn connect(&self) {
            connect(&self.store);
        }

        /// One pass; the poll plugin runs at its start.
        fn frame(&self) {
            self.ctx
                .run_ui(RawInput::default(), |_| {})
                .drop_without_applying_deltas();
        }

        fn preset(&self) -> presets::PresetStyles {
            installed_preset(&self.ctx)
        }
    }

    #[test]
    fn a_store_that_opens_later_applies_its_active_theme_once_ready() {
        let [stored, ..] = others();
        let rig = Rig::new();
        rig.stored(preset_theme(stored, Density::Regular));
        assert_ne!(rig.preset(), stored);
        assert_eq!(poll_store(&rig.ctx), StorePoll::Waiting);
        rig.frame();
        assert_ne!(
            rig.preset(),
            stored,
            "nothing is applied before the store loads"
        );

        rig.connect();
        assert_eq!(poll_store(&rig.ctx), StorePoll::Applied);
        assert_eq!(rig.preset(), stored);
        assert_eq!(persist::source(&rig.ctx), ThemeSource::Stored);
        assert_eq!(theme_epoch(&rig.ctx), 1);
        assert_eq!(poll_store(&rig.ctx), StorePoll::Idle);
    }

    #[test]
    fn the_per_frame_plugin_polls_without_any_app_call() {
        let [stored, ..] = others();
        let rig = Rig::new();
        rig.stored(preset_theme(stored, Density::Regular));
        rig.frame();
        rig.connect();
        rig.frame();
        assert_eq!(rig.preset(), stored);
    }

    #[test]
    fn applying_the_stored_theme_discards_the_pass_so_no_widget_draws_in_the_old_style() {
        let [stored, ..] = others();
        let rig = Rig::new();
        rig.stored(preset_theme(stored, Density::Regular));
        rig.connect();
        let passes_of = |rig: &Rig| {
            let output = rig.ctx.run_ui(RawInput::default(), |_| {});
            let passes = output.platform_output.num_completed_passes;
            output.drop_without_applying_deltas();
            passes
        };
        assert_eq!(passes_of(&rig), 2, "the applying pass was repeated");
        assert_eq!(passes_of(&rig), 1);
    }

    #[test]
    fn an_edit_made_before_the_store_was_ready_does_not_block_later_external_changes() {
        let [stored, chosen, elsewhere, _] = others();
        let rig = Rig::new();
        rig.stored(preset_theme(stored, Density::Regular));
        apply_preset(&rig.ctx, chosen);
        let mut autosave = AutoSave::new(std::time::Duration::ZERO);
        autosave.mark_clean(&rig.ctx);
        apply_preset(&rig.ctx, stored);
        apply_preset(&rig.ctx, chosen);
        autosave.update(&rig.ctx, &*rig.store);
        rig.connect();
        assert_eq!(poll_store(&rig.ctx), StorePoll::Kept);
        assert_eq!(rig.preset(), chosen);

        rig.stored(preset_theme(elsewhere, Density::Regular));
        rig.store.reload();
        rig.store.flush();
        assert_eq!(poll_store(&rig.ctx), StorePoll::Applied);
        assert_eq!(rig.preset(), elsewhere);
    }

    #[test]
    fn the_store_wakes_the_ui_when_the_load_finishes() {
        let rig = Rig::new();
        let wakes = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&wakes);
        rig.store.on_change(Arc::new(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        }));
        let before = wakes.load(Ordering::SeqCst);
        rig.connect();
        assert!(wakes.load(Ordering::SeqCst) > before);
    }

    #[test]
    fn a_matching_stored_theme_leaves_the_live_theme_untouched() {
        let [stored, ..] = others();
        let rig = Rig::new();
        apply_preset(&rig.ctx, stored);
        adopt_live_theme(&rig.ctx);
        rig.stored(SavedTheme::current(&rig.ctx));
        let style = rig.ctx.global_style();
        rig.connect();
        assert_eq!(poll_store(&rig.ctx), StorePoll::Kept);
        assert!(Arc::ptr_eq(&style, &rig.ctx.global_style()));
        assert_eq!(theme_epoch(&rig.ctx), 0);
    }

    #[test]
    fn a_change_made_before_the_store_was_ready_beats_the_stored_theme() {
        let [stored, chosen, ..] = others();
        let rig = Rig::new();
        rig.stored(preset_theme(stored, Density::Regular));
        apply_preset(&rig.ctx, chosen);
        rig.connect();
        assert_eq!(poll_store(&rig.ctx), StorePoll::Kept);
        assert_eq!(rig.preset(), chosen);
        assert_eq!(theme_epoch(&rig.ctx), 0);
    }

    #[test]
    fn a_theme_adopted_after_init_counts_as_the_startup_theme() {
        let [stored, migrated, ..] = others();
        let rig = Rig::new();
        rig.stored(preset_theme(stored, Density::Regular));
        apply_preset(&rig.ctx, migrated);
        adopt_live_theme(&rig.ctx);
        rig.connect();
        assert_eq!(poll_store(&rig.ctx), StorePoll::Applied);
        assert_eq!(rig.preset(), stored);
    }

    #[test]
    fn an_empty_store_is_seeded_with_a_custom_live_theme_but_not_the_default() {
        let [chosen, ..] = others();
        let plain = Rig::new();
        plain.connect();
        assert_eq!(poll_store(&plain.ctx), StorePoll::Kept);
        assert!(plain.store.load_active().is_none());

        let rig = Rig::new();
        apply_preset(&rig.ctx, chosen);
        adopt_live_theme(&rig.ctx);
        rig.connect();
        assert_eq!(poll_store(&rig.ctx), StorePoll::Seeded);
        rig.store.flush();
        let written = rig.backend.rows.lock().unwrap().active.clone();
        assert_eq!(preset_of(written.expect("seeded")), chosen);
        assert_eq!(poll_store(&rig.ctx), StorePoll::Idle);
    }

    #[test]
    fn a_theme_file_run_is_never_replaced_or_seeded() {
        let [stored, file, ..] = others();
        let rig = Rig::new();
        rig.stored(preset_theme(stored, Density::Regular));
        apply_preset(&rig.ctx, file);
        persist::set_source(&rig.ctx, ThemeSource::File("theme.json".into()));
        adopt_live_theme(&rig.ctx);
        rig.connect();
        assert_eq!(poll_store(&rig.ctx), StorePoll::Kept);
        assert_eq!(rig.preset(), file);

        let [_, _, third, _] = others();
        rig.stored(preset_theme(third, Density::Regular));
        rig.store.reload();
        rig.store.flush();
        assert_eq!(poll_store(&rig.ctx), StorePoll::Idle);
        assert_eq!(rig.preset(), file);
        assert_eq!(theme_epoch(&rig.ctx), 0);
    }

    #[test]
    fn the_startup_pin_applies_once_and_later_choices_are_not_overridden() {
        let [pinned, active, chosen, _] = others();
        let rig = Rig::new();
        {
            let mut rows = rig.backend.rows.lock().unwrap();
            rows.active = Some(preset_theme(active, Density::Regular));
            rows.startup = StartupTheme::Preset(pinned.def().expect("built in").id.to_owned());
        }
        rig.connect();
        assert_eq!(poll_store(&rig.ctx), StorePoll::Applied);
        assert_eq!(rig.preset(), pinned);

        apply_preset(&rig.ctx, chosen);
        rig.store.save_active(&SavedTheme::current(&rig.ctx));
        rig.store.flush();
        assert_eq!(poll_store(&rig.ctx), StorePoll::Kept);
        assert_eq!(rig.preset(), chosen);
        assert_eq!(poll_store(&rig.ctx), StorePoll::Idle);
    }

    #[test]
    fn a_later_active_theme_from_elsewhere_applies_unless_there_are_local_edits() {
        let [first, elsewhere, local, other] = others();
        let rig = Rig::new();
        rig.stored(preset_theme(first, Density::Regular));
        rig.connect();
        assert_eq!(poll_store(&rig.ctx), StorePoll::Applied);

        rig.stored(preset_theme(elsewhere, Density::Regular));
        rig.store.reload();
        rig.store.flush();
        assert_eq!(poll_store(&rig.ctx), StorePoll::Applied);
        assert_eq!(rig.preset(), elsewhere);
        assert_eq!(theme_epoch(&rig.ctx), 2);

        apply_preset(&rig.ctx, local);
        rig.stored(preset_theme(other, Density::Regular));
        rig.store.reload();
        rig.store.flush();
        assert_eq!(poll_store(&rig.ctx), StorePoll::Kept);
        assert_eq!(
            rig.preset(),
            local,
            "an unsaved local edit is not overwritten"
        );
    }

    #[test]
    fn saving_the_live_theme_to_the_store_is_not_an_external_change() {
        let [first, chosen, ..] = others();
        let rig = Rig::new();
        rig.stored(preset_theme(first, Density::Regular));
        rig.connect();
        poll_store(&rig.ctx);

        apply_preset(&rig.ctx, chosen);
        let mut autosave = AutoSave::new(std::time::Duration::ZERO);
        autosave.mark_clean(&rig.ctx);
        apply_preset(&rig.ctx, first);
        apply_preset(&rig.ctx, chosen);
        autosave.update(&rig.ctx, &*rig.store);
        autosave.update(&rig.ctx, &*rig.store);
        rig.store.flush();
        let epoch = theme_epoch(&rig.ctx);
        assert_eq!(poll_store(&rig.ctx), StorePoll::Kept);
        assert_eq!(rig.preset(), chosen);
        assert_eq!(theme_epoch(&rig.ctx), epoch);
    }

    #[test]
    fn a_theme_applied_by_the_poll_is_not_saved_back() {
        let [stored, ..] = others();
        let rig = Rig::new();
        let mut autosave = AutoSave::new(std::time::Duration::ZERO);
        autosave.update(&rig.ctx, &*rig.store);
        rig.stored(preset_theme(stored, Density::Regular));
        rig.connect();
        assert_eq!(poll_store(&rig.ctx), StorePoll::Applied);
        let writes = rig.store.revision();
        autosave.update(&rig.ctx, &*rig.store);
        autosave.update(&rig.ctx, &*rig.store);
        assert!(!autosave.is_pending());
        assert_eq!(
            rig.store.revision(),
            writes,
            "the applied theme was adopted, not written"
        );
    }

    #[test]
    fn an_empty_load_after_a_reset_to_default_keeps_the_live_theme() {
        let [first, ..] = others();
        let rig = Rig::new();
        rig.stored(preset_theme(first, Density::Regular));
        rig.connect();
        poll_store(&rig.ctx);

        rig.backend.rows.lock().unwrap().active = None;
        rig.store.reload();
        rig.store.flush();
        assert_eq!(poll_store(&rig.ctx), StorePoll::Kept);
        assert_eq!(rig.preset(), first);
    }

    #[test]
    fn a_store_attached_after_init_is_polled_like_a_configured_one() {
        let [stored, ..] = others();
        let ctx = Context::default();
        init_context(&ctx, None, KitConfig::new().parse_cli(false));
        assert_eq!(poll_store(&ctx), StorePoll::Idle);

        let store = Arc::new(MemoryStore::with_snapshot(crate::store::StoreSnapshot {
            active: Some(preset_theme(stored, Density::Regular)),
            ..Default::default()
        }));
        attach_store(&ctx, store);
        assert_eq!(poll_store(&ctx), StorePoll::Applied);
        assert_eq!(installed_preset(&ctx), stored);
    }

    #[test]
    fn a_failed_first_load_keeps_the_store_waiting_and_a_later_one_succeeds() {
        let [stored, ..] = others();
        let rig = Rig::new();
        rig.stored(preset_theme(stored, Density::Regular));
        rig.backend.offline.store(true, Ordering::SeqCst);
        rig.connect();
        assert!(rig.store.last_error().is_some());
        assert_eq!(poll_store(&rig.ctx), StorePoll::Waiting);

        rig.backend.offline.store(false, Ordering::SeqCst);
        rig.connect();
        assert_eq!(poll_store(&rig.ctx), StorePoll::Applied);
        assert_eq!(rig.preset(), stored);
    }

    #[test]
    fn writes_made_before_the_store_was_ready_reach_the_backend_after_it_opens() {
        let [stored, chosen, ..] = others();
        let rig = Rig::new();
        rig.stored(preset_theme(stored, Density::Regular));
        rig.store
            .save_user_theme("Early", &preset_theme(chosen, Density::Regular));
        rig.store
            .save_active(&preset_theme(chosen, Density::Regular));
        assert_eq!(
            rig.store.user_themes().len(),
            1,
            "the cache shows it at once"
        );
        assert!(rig.backend.rows.lock().unwrap().user_themes.is_empty());

        rig.connect();
        let rows = rig.backend.rows.lock().unwrap().clone();
        assert_eq!(rows.user_themes.len(), 1);
        assert_eq!(preset_of(rows.active.expect("replayed")), chosen);
        assert_eq!(rig.store.user_themes().len(), 1);
        assert_eq!(rig.store.load_active().map(preset_of), Some(chosen));
    }
}
