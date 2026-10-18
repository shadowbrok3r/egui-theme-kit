//! One-call setup: command-line options, fonts, glass and the startup theme, configured by [`KitConfig`].
//!
//! # Store readiness
//!
//! The store given to [`KitConfig::store`] may open after the window does (an async database).
//! While [`ThemeStore::is_ready`] is false the kit starts from eframe storage or the default theme;
//! [`poll_store`] then runs every frame, through a plugin [`init`] registers, and acts on the store:
//!
//! - when the store first becomes ready it applies the pinned startup theme, else the active theme,
//!   unless the live theme was changed since init (the user's choice wins) or came from `--theme`;
//! - an empty ready store receives the live theme when it differs from the default;
//! - afterwards, a change of the store's active theme that did not come from this app (another
//!   window, a reload) is applied unless the live theme has unsaved local edits.
//!
//! A theme from the `--theme` file is never replaced. [`theme_epoch`] counts the themes the kit
//! applied this way so a saver such as [`AutoSave`](crate::store::AutoSave) does not write them back.

use std::fmt;
use std::sync::Arc;

use eframe::egui::{Context, Plugin, Theme, Ui};

use super::{
    cli, debug_panel,
    fonts::{self, FontConfig},
    glass,
    persist::{self, ThemeSource},
    presets::{MATCHABLE_PRESETS, apply_preset, apply_saved_theme, themes_equal},
    saved::SavedTheme,
};

mod sync;

pub use crate::store::ThemeStore;
pub use sync::{StorePoll, poll_store};

/// Settings handed to [`init`] and [`init_context`].
#[derive(Clone)]
pub struct KitConfig {
    /// Extra faces, monospace order and installer hooks for the font stack.
    pub fonts: FontConfig,
    /// Preset applied when nothing else supplies a theme, matched by name ignoring case.
    pub default_preset: Option<String>,
    /// Theme applied when nothing else supplies one; wins over `default_preset`.
    pub default_theme: Option<SavedTheme>,
    /// Persistent theme store; its pinned or active theme starts the app once it is ready.
    pub store: Option<Arc<dyn ThemeStore>>,
    /// Reserved; ignored. Temp-data ids use the `egui_theme_kit.` prefix.
    pub id_prefix: Option<String>,
    /// Reads `--theme`, `--theme-dump`, `--theme-panel` and `--no-frost` from the process arguments when [`cli::install`] was not called.
    pub parse_cli: bool,
    /// Eframe storage keys an earlier release wrote the theme under; read after the current key and removed on save.
    pub legacy_storage_keys: Vec<String>,
}

impl Default for KitConfig {
    fn default() -> Self {
        Self {
            fonts: FontConfig::default(),
            default_preset: None,
            default_theme: None,
            store: None,
            id_prefix: None,
            parse_cli: true,
            legacy_storage_keys: Vec::new(),
        }
    }
}

impl fmt::Debug for KitConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KitConfig")
            .field("fonts", &self.fonts)
            .field("default_preset", &self.default_preset)
            .field("default_theme", &self.default_theme.is_some())
            .field("store", &self.store.is_some())
            .field("parse_cli", &self.parse_cli)
            .field("legacy_storage_keys", &self.legacy_storage_keys)
            .finish_non_exhaustive()
    }
}

impl KitConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn fonts(mut self, fonts: FontConfig) -> Self {
        self.fonts = fonts;
        self
    }

    pub fn default_preset(mut self, name: impl Into<String>) -> Self {
        self.default_preset = Some(name.into());
        self
    }

    pub fn default_theme(mut self, theme: SavedTheme) -> Self {
        self.default_theme = Some(theme);
        self
    }

    pub fn store(mut self, store: Arc<dyn ThemeStore>) -> Self {
        self.store = Some(store);
        self
    }

    /// Wraps a store of the removed two-method trait; only the active theme persists through it.
    #[deprecated(note = "implement `store::ThemeStore` and pass it to `KitConfig::store`")]
    #[allow(deprecated)]
    pub fn legacy_store(mut self, store: Arc<dyn store_stub::ThemeStore>) -> Self {
        let adapter: Arc<dyn ThemeStore> = Arc::new(store_stub::LegacyStore::new(store));
        self.store = Some(adapter);
        self
    }

    pub fn parse_cli(mut self, parse: bool) -> Self {
        self.parse_cli = parse;
        self
    }

    /// Also reads (and, on save, removes) the eframe storage entry an earlier release used for the theme.
    pub fn legacy_storage_key(mut self, key: impl Into<String>) -> Self {
        self.legacy_storage_keys.push(key.into());
        self
    }
}

/// Installs the glass backend, fonts and the startup theme; the source order is in the crate docs.
pub fn init(cc: &eframe::CreationContext<'_>, cfg: KitConfig) {
    if cfg.parse_cli {
        cli::ensure_installed();
    }
    glass::install(cc);
    init_context(&cc.egui_ctx, cc.storage, cfg);
}

/// [`init`] without the GL backend: command-line options, fonts and the startup theme on `ctx`.
pub fn init_context(ctx: &Context, storage: Option<&dyn eframe::Storage>, cfg: KitConfig) {
    if cfg.parse_cli {
        cli::ensure_installed();
    }
    ctx.set_theme(Theme::Dark);
    let default = configured_default(&cfg);
    set_state(
        ctx,
        KitState {
            default_theme: default.clone(),
            store: None,
            legacy_keys: cfg.legacy_storage_keys.clone(),
            sync: sync::SyncState::default(),
        },
    );
    fonts::set_config(ctx, cfg.fonts);
    fonts::install_for_context(ctx);

    let options = cli::options();
    let (theme, source) = startup_theme(
        storage,
        options.theme_path.as_deref(),
        cfg.store.as_deref(),
        default.as_ref(),
        &cfg.legacy_storage_keys,
    );
    log::info!("theme: applying {}", source.label());
    persist::set_source(ctx, source);
    apply_saved_theme(ctx, theme);
    if let Some(store) = cfg.store {
        let applied = store.is_ready();
        install_store(ctx, store, applied);
    }
    if options.theme_panel {
        debug_panel::set_open(ctx, true);
    }
}

/// Registers `store` after [`init`] and lets [`poll_store`] apply its theme when it is ready.
///
/// For apps that build the store after the window opens. The live theme at this moment counts as the
/// startup theme: a later change by the user is kept over the store's content.
pub fn attach_store(ctx: &Context, store: Arc<dyn ThemeStore>) {
    install_store(ctx, store, false);
}

/// Treats the live theme as the startup theme, so the store's first read does not mistake it for a user change.
///
/// Call after applying a theme of your own right after [`init`], such as one migrated from older settings.
pub fn adopt_live_theme(ctx: &Context) {
    let live = SavedTheme::current(ctx);
    ctx.with_plugin::<KitState, _>(|state| state.sync.synced = Some(live));
}

/// How many themes [`poll_store`] has applied to `ctx`; savers compare it to skip writing them back.
pub fn theme_epoch(ctx: &Context) -> u64 {
    ctx.with_plugin::<KitState, _>(|state| state.sync.epoch)
        .unwrap_or(0)
}

/// The store registered by [`init`], [`init_context`] or [`attach_store`].
pub fn store(ctx: &Context) -> Option<Arc<dyn ThemeStore>> {
    ctx.with_plugin::<KitState, _>(|state| state.store.clone())
        .flatten()
}

/// The default theme resolved from the config at init.
pub fn default_theme(ctx: &Context) -> Option<SavedTheme> {
    ctx.with_plugin::<KitState, _>(|state| state.default_theme.clone())
        .flatten()
}

/// Whether `theme` equals the configured default in every part; false when none was configured.
pub fn is_configured_default(ctx: &Context, theme: &SavedTheme) -> bool {
    default_theme(ctx).is_some_and(|default| themes_equal(&default, theme))
}

/// The legacy eframe storage keys registered by the config.
pub(crate) fn legacy_storage_keys(ctx: &Context) -> Vec<String> {
    ctx.with_plugin::<KitState, _>(|state| state.legacy_keys.clone())
        .unwrap_or_default()
}

/// Plugin holding the resolved config so a memory reset leaves it.
struct KitState {
    default_theme: Option<SavedTheme>,
    store: Option<Arc<dyn ThemeStore>>,
    legacy_keys: Vec<String>,
    sync: sync::SyncState,
}

impl Plugin for KitState {
    fn debug_name(&self) -> &'static str {
        "egui_theme_kit::startup::KitState"
    }

    fn on_exit(&mut self, _ctx: &Context) {
        self.store = None;
    }
}

/// Plugin that polls the registered store at the start of every pass.
struct StorePoller;

impl Plugin for StorePoller {
    fn debug_name(&self) -> &'static str {
        "egui_theme_kit::startup::StorePoller"
    }

    fn on_begin_pass(&mut self, ui: &mut Ui) {
        poll_store(ui.ctx());
    }
}

fn set_state(ctx: &Context, state: KitState) {
    let KitState {
        default_theme,
        store,
        legacy_keys,
        sync,
    } = state;
    let replaced = ctx.with_plugin::<KitState, _>(|slot| {
        slot.default_theme = default_theme.clone();
        slot.store = store.clone();
        slot.legacy_keys = legacy_keys.clone();
        slot.sync = sync.clone();
    });
    if replaced.is_none() {
        ctx.add_plugin(KitState {
            default_theme,
            store,
            legacy_keys,
            sync,
        });
    }
}

/// Registers `store`, its repaint wake-up and the per-frame poll; `applied` says the startup theme already came from it.
fn install_store(ctx: &Context, store: Arc<dyn ThemeStore>, applied: bool) {
    let known = ctx
        .with_plugin::<KitState, _>(|state| {
            state
                .store
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, &store))
        })
        .unwrap_or(false);
    if !known {
        let repaint = ctx.clone();
        store.on_change(Arc::new(move || repaint.request_repaint()));
    }
    let live = SavedTheme::current(ctx);
    let sync = sync::SyncState::attached(ctx, &*store, live, applied);
    let attached = ctx.with_plugin::<KitState, _>(|state| {
        state.store = Some(Arc::clone(&store));
        state.sync = sync.clone();
    });
    if attached.is_none() {
        set_state(
            ctx,
            KitState {
                default_theme: None,
                store: Some(store),
                legacy_keys: Vec::new(),
                sync,
            },
        );
    }
    ctx.add_plugin(StorePoller);
}

/// `default_theme`, else the theme of `default_preset`, else the deprecated process-wide default, else none.
fn configured_default(cfg: &KitConfig) -> Option<SavedTheme> {
    if let Some(theme) = &cfg.default_theme {
        return Some(theme.clone());
    }
    if let Some(name) = cfg.default_preset.as_deref() {
        let found = MATCHABLE_PRESETS
            .into_iter()
            .find(|p| p.as_str().eq_ignore_ascii_case(name.trim()));
        match found {
            Some(preset) => {
                let scratch = Context::default();
                apply_preset(&scratch, preset);
                return Some(SavedTheme::current(&scratch));
            }
            None => log::warn!("default preset {name:?} is unknown; using the shipped default"),
        }
    }
    persist::global_default()
}

/// The first available startup theme: file, ready store, eframe storage, configured default, shipped default.
fn startup_theme(
    storage: Option<&dyn eframe::Storage>,
    file: Option<&std::path::Path>,
    store: Option<&dyn ThemeStore>,
    default: Option<&SavedTheme>,
    legacy_keys: &[String],
) -> (SavedTheme, ThemeSource) {
    if let Some(path) = file {
        match persist::load_file(path) {
            Ok(theme) => return (theme, ThemeSource::File(path.to_path_buf())),
            Err(e) => log::warn!(
                "--theme {}: {e}; using another theme source",
                path.display()
            ),
        }
    }
    if let Some(theme) = store
        .filter(|store| store.is_ready())
        .and_then(persist::store_theme)
    {
        return (theme, ThemeSource::Stored);
    }
    let keys: Vec<&str> = std::iter::once(persist::STORAGE_KEY)
        .chain(legacy_keys.iter().map(String::as_str))
        .collect();
    if let Some(theme) = persist::load_stored_from(storage, &keys) {
        return (theme, ThemeSource::Stored);
    }
    match default {
        Some(theme) => (theme.clone(), ThemeSource::Embedded),
        None => (persist::shipped_default(), ThemeSource::Embedded),
    }
}

/// Stand-in of the removed two-method store trait, kept so older implementations still compile.
pub mod store_stub {
    use std::sync::Arc;

    use crate::saved::SavedTheme;
    use crate::store::{
        ChangeCallback, MemoryStore, NamedTheme, StartupTheme, StoreSnapshot, ThemeStore as Store,
    };

    /// Where the active theme persists between runs.
    #[deprecated(
        note = "implement `store::ThemeStore`; wrap an old implementation with `KitConfig::legacy_store`"
    )]
    pub trait ThemeStore: Send + Sync {
        /// The active theme, or none when nothing was saved.
        fn load_active(&self) -> Option<SavedTheme>;
        /// Records `theme` as the active theme.
        fn save_active(&self, theme: &SavedTheme);
    }

    /// A full [`store::ThemeStore`](Store) over a two-method store: the active theme persists through it, everything else stays in memory.
    #[deprecated(note = "implement `store::ThemeStore`")]
    #[allow(deprecated)]
    pub struct LegacyStore {
        inner: Arc<dyn ThemeStore>,
        memory: MemoryStore,
    }

    #[allow(deprecated)]
    impl LegacyStore {
        /// Wraps `inner`, seeding the cache with its active theme.
        pub fn new(inner: Arc<dyn ThemeStore>) -> Self {
            let snapshot = StoreSnapshot {
                active: inner.load_active(),
                ..StoreSnapshot::default()
            };
            Self {
                inner,
                memory: MemoryStore::with_snapshot(snapshot),
            }
        }
    }

    #[allow(deprecated)]
    impl Store for LegacyStore {
        fn load_active(&self) -> Option<SavedTheme> {
            self.memory.load_active()
        }

        fn save_active(&self, theme: &SavedTheme) {
            self.memory.save_active(theme);
            self.inner.save_active(theme);
        }

        fn clear_active(&self) {
            self.memory.clear_active();
        }

        fn user_themes(&self) -> Vec<NamedTheme> {
            self.memory.user_themes()
        }

        fn save_user_theme(&self, name: &str, theme: &SavedTheme) {
            self.memory.save_user_theme(name, theme);
        }

        fn delete_user_theme(&self, name: &str) {
            self.memory.delete_user_theme(name);
        }

        fn rename_user_theme(&self, from: &str, to: &str) {
            self.memory.rename_user_theme(from, to);
        }

        fn startup(&self) -> StartupTheme {
            self.memory.startup()
        }

        fn set_startup(&self, startup: StartupTheme) {
            self.memory.set_startup(startup);
        }

        fn revision(&self) -> u64 {
            self.memory.revision()
        }

        fn last_error(&self) -> Option<String> {
            self.memory.last_error()
        }

        fn clear_error(&self) {
            self.memory.clear_error();
        }

        fn on_change(&self, callback: ChangeCallback) {
            self.memory.on_change(callback);
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use eframe::Storage as _;
    use std::collections::HashMap;
    use std::path::PathBuf;

    use super::super::{
        metrics::Density,
        presets::{self, PresetStyles, installed_preset},
        saved::theme_to_json,
        store::{CachedStore, MemoryStore, StartupTheme, StoreSnapshot, testing::SharedBackend},
    };

    #[derive(Default)]
    pub(crate) struct MemStorage(pub HashMap<String, String>);

    impl eframe::Storage for MemStorage {
        fn get_string(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }

        fn set_string(&mut self, key: &str, value: String) {
            self.0.insert(key.to_owned(), value);
        }

        fn remove_string(&mut self, key: &str) {
            self.0.remove(key);
        }

        fn flush(&mut self) {}
    }

    pub(crate) fn store_with(theme: Option<SavedTheme>) -> MemoryStore {
        MemoryStore::with_snapshot(StoreSnapshot {
            active: theme,
            ..Default::default()
        })
    }

    pub(crate) fn preset_theme(preset: PresetStyles, density: Density) -> SavedTheme {
        let ctx = Context::default();
        apply_preset(&ctx, preset);
        presets::set_density(&ctx, density);
        SavedTheme::current(&ctx)
    }

    /// Four distinct presets that are not the shipped default.
    pub(crate) fn others() -> [PresetStyles; 4] {
        let mut presets = MATCHABLE_PRESETS
            .into_iter()
            .filter(|p| *p != presets::TOP_PRESET);
        std::array::from_fn(|_| presets.next().expect("at least four non-top presets"))
    }

    fn temp_theme_file(tag: &str, theme: &SavedTheme) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("egui_theme_kit_init_{tag}_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("theme.json");
        std::fs::write(&path, theme_to_json(theme).unwrap()).unwrap();
        path
    }

    pub(crate) fn preset_of(theme: SavedTheme) -> PresetStyles {
        let ctx = Context::default();
        apply_saved_theme(&ctx, theme);
        installed_preset(&ctx)
    }

    #[test]
    fn the_startup_order_is_file_store_storage_default_then_shipped() {
        let [file_preset, store_preset, stored_preset, default_preset] = others();
        let file_theme = preset_theme(file_preset, Density::Regular);
        let store_theme = preset_theme(store_preset, Density::Regular);
        let stored_theme = preset_theme(stored_preset, Density::Regular);
        let default_theme = preset_theme(default_preset, Density::Regular);

        let path = temp_theme_file("order", &file_theme);
        let mut storage = MemStorage::default();
        let ctx = Context::default();
        apply_saved_theme(&ctx, stored_theme);
        persist::save(&ctx, &mut storage);
        let store = store_with(Some(store_theme));

        let pick = |file: Option<&std::path::Path>,
                    store: Option<&dyn ThemeStore>,
                    storage: Option<&dyn eframe::Storage>,
                    default: Option<&SavedTheme>| {
            let (theme, source) = startup_theme(storage, file, store, default, &[]);
            (preset_of(theme), source)
        };

        let (preset, source) = pick(
            Some(&path),
            Some(&store),
            Some(&storage),
            Some(&default_theme),
        );
        assert_eq!(
            (preset, source),
            (file_preset, ThemeSource::File(path.clone()))
        );

        let (preset, source) = pick(None, Some(&store), Some(&storage), Some(&default_theme));
        assert_eq!((preset, source), (store_preset, ThemeSource::Stored));

        let (preset, source) = pick(None, None, Some(&storage), Some(&default_theme));
        assert_eq!((preset, source), (stored_preset, ThemeSource::Stored));

        let (preset, source) = pick(None, None, None, Some(&default_theme));
        assert_eq!((preset, source), (default_preset, ThemeSource::Embedded));

        let (_, source) = pick(None, None, None, None);
        assert_eq!(source, ThemeSource::Embedded);

        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn a_missing_file_and_an_empty_store_fall_through() {
        let [_, store_preset, ..] = others();
        let missing = PathBuf::from("/nonexistent/egui_theme_kit/theme.json");
        let default = preset_theme(store_preset, Density::Regular);
        let empty = store_with(None);
        let (theme, source) =
            startup_theme(None, Some(&missing), Some(&empty), Some(&default), &[]);
        assert_eq!(source, ThemeSource::Embedded);
        assert_eq!(preset_of(theme), store_preset);
    }

    #[test]
    fn a_store_that_is_not_ready_is_skipped_at_startup() {
        let [_, store_preset, default_preset, _] = others();
        let store = CachedStore::open_lazy(SharedBackend::default());
        store.save_active(&preset_theme(store_preset, Density::Regular));
        let default = preset_theme(default_preset, Density::Regular);
        let (theme, source) = startup_theme(None, None, Some(&store), Some(&default), &[]);
        assert_eq!(source, ThemeSource::Embedded);
        assert_eq!(preset_of(theme), default_preset);
    }

    #[test]
    fn the_startup_pin_of_a_ready_store_beats_its_active_theme() {
        let [pinned, active, ..] = others();
        let store = store_with(Some(preset_theme(active, Density::Regular)));
        store.set_startup(StartupTheme::Preset(
            pinned.def().expect("built in").id.to_owned(),
        ));
        let ctx = Context::default();
        init_context(
            &ctx,
            None,
            KitConfig::new().parse_cli(false).store(Arc::new(store)),
        );
        assert_eq!(installed_preset(&ctx), pinned);
    }

    #[test]
    fn legacy_storage_keys_are_read_after_the_current_one_and_removed_on_save() {
        let [preset, ..] = others();
        let mut storage = MemStorage::default();
        let json = theme_to_json(&preset_theme(preset, Density::Regular)).unwrap();
        storage.set_string("earlier.theme", String::from_utf8(json).unwrap());
        let cfg = || {
            KitConfig::new()
                .parse_cli(false)
                .legacy_storage_key("earlier.theme")
        };

        let ctx = Context::default();
        init_context(&ctx, Some(&storage), cfg());
        assert_eq!(installed_preset(&ctx), preset);
        assert_eq!(persist::source(&ctx), ThemeSource::Stored);

        crate::save(&ctx, &mut storage);
        assert!(storage.get_string(persist::STORAGE_KEY).is_some());
        assert_eq!(storage.get_string("earlier.theme"), None);

        let next = Context::default();
        init_context(&next, Some(&storage), cfg());
        assert_eq!(installed_preset(&next), preset);
    }

    #[test]
    fn init_applies_the_configured_default_and_registers_the_config() {
        let [preset, ..] = others();
        let ctx = Context::default();
        let cfg = KitConfig::new()
            .parse_cli(false)
            .default_preset(preset.as_str().to_uppercase())
            .fonts(FontConfig::new().face("probe", &[0, 1, 2, 3]));
        init_context(&ctx, None, cfg);

        assert_eq!(ctx.theme(), Theme::Dark);
        assert_eq!(installed_preset(&ctx), preset);
        assert_eq!(persist::source(&ctx), ThemeSource::Embedded);
        assert_eq!(fonts::config(&ctx).faces.len(), 1);
        assert!(default_theme(&ctx).is_some());
        assert!(is_configured_default(&ctx, &SavedTheme::current(&ctx)));

        ctx.memory_mut(|mem| *mem = Default::default());
        assert!(
            default_theme(&ctx).is_some(),
            "the config survives a memory reset"
        );
    }

    #[test]
    fn an_explicit_default_theme_beats_the_default_preset() {
        let [first, second, ..] = others();
        let ctx = Context::default();
        let cfg = KitConfig::new()
            .parse_cli(false)
            .default_preset(first.as_str())
            .default_theme(preset_theme(second, Density::Comfortable));
        init_context(&ctx, None, cfg);
        assert_eq!(installed_preset(&ctx), second);
        assert_eq!(metrics_density(&ctx), Density::Comfortable);
    }

    fn metrics_density(ctx: &Context) -> Density {
        super::super::metrics::density(ctx)
    }

    #[test]
    fn an_unknown_default_preset_falls_back_to_the_shipped_default() {
        let ctx = Context::default();
        init_context(
            &ctx,
            None,
            KitConfig::new()
                .parse_cli(false)
                .default_preset("no such preset"),
        );
        assert!(default_theme(&ctx).is_none());
        assert_eq!(persist::source(&ctx), ThemeSource::Embedded);
    }

    #[test]
    fn a_ready_empty_store_receives_the_theme_restored_from_eframe_storage() {
        let [preset, ..] = others();
        let mut storage = MemStorage::default();
        let json = theme_to_json(&preset_theme(preset, Density::Regular)).unwrap();
        storage.set_string(persist::STORAGE_KEY, String::from_utf8(json).unwrap());
        let store = Arc::new(store_with(None));
        let ctx = Context::default();
        init_context(
            &ctx,
            Some(&storage),
            KitConfig::new().parse_cli(false).store(store.clone()),
        );
        assert_eq!(installed_preset(&ctx), preset);
        assert_eq!(preset_of(store.load_active().expect("seeded")), preset);

        let plain = Arc::new(store_with(None));
        init_context(
            &Context::default(),
            None,
            KitConfig::new().parse_cli(false).store(plain.clone()),
        );
        assert!(
            plain.load_active().is_none(),
            "the default theme is not written"
        );
    }

    #[test]
    fn the_store_theme_is_applied_and_reachable_from_the_context() {
        let [preset, ..] = others();
        let store = Arc::new(store_with(Some(preset_theme(preset, Density::Regular))));
        let ctx = Context::default();
        init_context(
            &ctx,
            None,
            KitConfig::new().parse_cli(false).store(store.clone()),
        );
        assert_eq!(installed_preset(&ctx), preset);
        assert_eq!(persist::source(&ctx), ThemeSource::Stored);
        let registered = super::store(&ctx).expect("the store is registered");
        assert!(registered.load_active().is_some());
    }

    #[test]
    #[allow(deprecated)]
    fn an_old_two_method_store_still_plugs_in_through_the_adapter() {
        use std::sync::Mutex;

        struct Old(Mutex<Option<SavedTheme>>);

        impl store_stub::ThemeStore for Old {
            fn load_active(&self) -> Option<SavedTheme> {
                self.0.lock().unwrap().clone()
            }

            fn save_active(&self, theme: &SavedTheme) {
                *self.0.lock().unwrap() = Some(theme.clone());
            }
        }

        let [preset, other, ..] = others();
        let old = Arc::new(Old(Mutex::new(Some(preset_theme(
            preset,
            Density::Regular,
        )))));
        let ctx = Context::default();
        init_context(
            &ctx,
            None,
            KitConfig::new().parse_cli(false).legacy_store(old.clone()),
        );
        assert_eq!(installed_preset(&ctx), preset);

        let registered = super::store(&ctx).expect("registered");
        registered.save_active(&preset_theme(other, Density::Regular));
        let kept = old.0.lock().unwrap().clone().expect("written through");
        assert_eq!(preset_of(kept), other);
    }

    #[test]
    fn an_app_whose_default_is_not_graphite_keeps_graphite_as_a_real_choice() {
        use super::super::{store::AutoSave, theme_chrome::graphite_theme};
        use std::time::Duration;

        let [app_default, ..] = others();
        let config = |store: Arc<MemoryStore>| {
            KitConfig::new()
                .parse_cli(false)
                .default_preset(app_default.as_str())
                .store(store)
        };
        let store = Arc::new(MemoryStore::default());
        let mut storage = MemStorage::default();

        let ctx = Context::default();
        init_context(&ctx, None, config(store.clone()));
        assert_eq!(installed_preset(&ctx), app_default);
        let mut autosave = AutoSave::new(Duration::ZERO);
        autosave.mark_clean(&ctx);

        apply_saved_theme(&ctx, graphite_theme());
        autosave.update(&ctx, &*store);
        let kept = store.load_active().expect("Graphite is saved, not cleared");
        assert_eq!(preset_of(kept), PresetStyles::Graphite);
        super::super::save(&ctx, &mut storage);
        assert!(storage.get_string(persist::STORAGE_KEY).is_some());
        assert!(!persist::is_default_in(&ctx, &SavedTheme::current(&ctx)));

        let next = Context::default();
        init_context(&next, Some(&storage), config(store.clone()));
        assert_eq!(installed_preset(&next), PresetStyles::Graphite);
        assert_eq!(persist::source(&next), ThemeSource::Stored);

        let back = SavedTheme::current(&ctx);
        apply_saved_theme(&ctx, persist::default_theme_in(&ctx));
        assert_eq!(installed_preset(&ctx), app_default);
        autosave.update(&ctx, &*store);
        assert!(
            store.load_active().is_none(),
            "the app default clears the entry"
        );
        super::super::save(&ctx, &mut storage);
        assert_eq!(storage.get_string(persist::STORAGE_KEY), None);
        assert!(!themes_equal(&back, &SavedTheme::current(&ctx)));
    }

    #[test]
    fn saved_storage_is_cleared_when_the_theme_is_the_configured_default() {
        let [preset, ..] = others();
        let ctx = Context::default();
        init_context(
            &ctx,
            None,
            KitConfig::new()
                .parse_cli(false)
                .default_preset(preset.as_str()),
        );
        let mut storage = MemStorage::default();
        storage.set_string(persist::STORAGE_KEY, "stale".to_owned());
        super::super::save(&ctx, &mut storage);
        assert_eq!(storage.get_string(persist::STORAGE_KEY), None);

        apply_preset(&ctx, others()[3]);
        super::super::save(&ctx, &mut storage);
        assert!(storage.get_string(persist::STORAGE_KEY).is_some());
    }
}
