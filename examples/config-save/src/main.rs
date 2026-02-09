//! Save and load key-value settings using the Config module.

#![no_std]
#![no_main]

use psp::config::{Config, ConfigValue};

psp::module!("config_save_example", 1, 1);

fn psp_main() {
    psp::enable_home_button();

    // Create a config and populate it.
    let mut cfg = Config::new();
    cfg.set("fullscreen", ConfigValue::Bool(true));
    cfg.set("volume", ConfigValue::I32(80));
    cfg.set("gamma", ConfigValue::F32(1.2));
    cfg.set("player_name", ConfigValue::Str("PSP_User".into()));

    psp::dprintln!("Created config with {} entries", cfg.len());

    // Save to file.
    let path = "host0:/test_config.rcfg";
    match cfg.save(path) {
        Ok(()) => psp::dprintln!("Saved config to {}", path),
        Err(e) => {
            psp::dprintln!("Failed to save: {:?}", e);
            return;
        },
    }

    // Load it back.
    let loaded = match Config::load(path) {
        Ok(c) => c,
        Err(e) => {
            psp::dprintln!("Failed to load: {:?}", e);
            return;
        },
    };

    psp::dprintln!("Loaded {} entries:", loaded.len());
    if let Some(v) = loaded.get_bool("fullscreen") {
        psp::dprintln!("  fullscreen = {}", v);
    }
    if let Some(v) = loaded.get_i32("volume") {
        psp::dprintln!("  volume = {}", v);
    }
    if let Some(v) = loaded.get_str("player_name") {
        psp::dprintln!("  player_name = {}", v);
    }
}
