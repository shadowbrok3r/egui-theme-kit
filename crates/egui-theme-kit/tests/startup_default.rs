//! The deprecated process-wide default theme is process state, so it is exercised alone in this test binary.
#![allow(deprecated)]

use std::sync::Mutex;

use eframe::egui::Context;
use egui_theme_kit::{
    KitConfig,
    metrics::Density,
    persist::{self, ThemeSource},
    presets::{self, PresetStyles},
    startup,
    store::{MemoryStore, ThemeStore},
};

/// Serializes the tests that touch the process-wide default.
static GLOBAL: Mutex<()> = Mutex::new(());

#[test]
fn an_app_default_theme_replaces_graphite_until_it_is_reset() {
    let _guard = GLOBAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut noir = PresetStyles::Noir.def().expect("built in").theme();
    noir.density = Density::Comfortable;

    let ctx = Context::default();
    presets::bootstrap_startup_theme(&ctx);
    assert_eq!(presets::installed_preset(&ctx), PresetStyles::Graphite);

    persist::set_default_theme(noir.clone());
    let fresh = Context::default();
    presets::bootstrap_startup_theme(&fresh);
    assert_eq!(presets::installed_preset(&fresh), PresetStyles::Noir);
    assert!(persist::is_embedded_default(&noir));
    assert_eq!(persist::resolve(None, None).1, ThemeSource::Embedded);

    let store = MemoryStore::default();
    let (theme, source) = persist::resolve_with_store(&store, None);
    assert_eq!(source, ThemeSource::Embedded);
    assert!(presets::themes_equal(&theme, &noir));

    store.save_active(&PresetStyles::Quartz.def().expect("built in").theme());
    assert_eq!(
        persist::resolve_with_store(&store, None).1,
        ThemeSource::Stored
    );

    persist::reset_default_theme();
    assert!(!persist::is_embedded_default(&noir));
    let again = Context::default();
    presets::bootstrap_startup_theme(&again);
    assert_eq!(presets::installed_preset(&again), PresetStyles::Graphite);
}

#[test]
fn the_deprecated_global_feeds_init_and_the_context_decisions_follow_it() {
    let _guard = GLOBAL.lock().unwrap_or_else(|e| e.into_inner());
    let noir = PresetStyles::Noir.def().expect("built in").theme();
    persist::set_default_theme(noir.clone());

    let ctx = Context::default();
    startup::init_context(&ctx, None, KitConfig::new().parse_cli(false));
    assert_eq!(presets::installed_preset(&ctx), PresetStyles::Noir);
    assert!(startup::is_configured_default(&ctx, &noir));
    assert!(persist::is_default_in(&ctx, &noir));

    let explicit = Context::default();
    startup::init_context(
        &explicit,
        None,
        KitConfig::new()
            .parse_cli(false)
            .default_preset(PresetStyles::Quartz.as_str()),
    );
    assert_eq!(presets::installed_preset(&explicit), PresetStyles::Quartz);
    assert!(
        !persist::is_default_in(&explicit, &noir),
        "the config default wins over the process-wide one"
    );

    persist::reset_default_theme();
    let plain = Context::default();
    startup::init_context(&plain, None, KitConfig::new().parse_cli(false));
    assert_eq!(presets::installed_preset(&plain), PresetStyles::Graphite);
}
