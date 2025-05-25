// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0
#![warn(clippy::all, rust_2018_idioms)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result<()> {
    use clap::Parser;

    #[derive(Debug, Parser)]
    struct Cli {
        /// The server WebSocket url.
        #[arg(long, short, default_value = "ws://127.0.0.1:9871")]
        url: String,
        /// The configuration storage key.
        #[arg(long, short)]
        storage: Option<String>,
    }

    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .format_target(false)
        .format_timestamp_millis()
        .init();

    let init_size = [1024.0, 640.0];
    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size(init_size)
            .with_min_inner_size(init_size)
            .with_max_inner_size(init_size)
            .with_title("Cards"),
        ..Default::default()
    };

    let cli = Cli::parse();

    let config = freezeout_gui::Config {
        server_url: cli.url,
    };

    let app_name = cli
        .storage
        .map(|s| format!("freezeout-{s}"))
        .unwrap_or_else(|| "freezeout".to_string());

    eframe::run_native(
        &app_name,
        native_options,
        Box::new(|cc| Ok(Box::new(freezeout_gui::AppFrame::new(config, cc)))),
    )
}

#[cfg(target_arch = "wasm32")]
fn main() {
    use eframe::wasm_bindgen::JsCast as _;

    eframe::WebLogger::init(log::LevelFilter::Debug).ok();

    wasm_bindgen_futures::spawn_local(async {
        let document = web_sys::window()
            .expect("No window")
            .document()
            .expect("No document");

        let canvas = document
            .get_element_by_id("canvas")
            .expect("Failed to find canvas element")
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .expect("canvas was not a HtmlCanvasElement");

        let server_url = document
            .get_element_by_id("server-url")
            .expect("Failed to find server-address element")
            .inner_html();

        let config = freezeout_gui::Config { server_url };

        eframe::WebRunner::new()
            .start(
                canvas,
                Default::default(),
                Box::new(|cc| Ok(Box::new(freezeout_gui::AppFrame::new(config, cc)))),
            )
            .await
            .expect("failed to start eframe");
    });
}
