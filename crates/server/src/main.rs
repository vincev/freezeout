// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0
use clap::Parser;
use freezeout_server::server;
use log::error;

#[derive(Debug, Parser)]
struct Cli {
    /// The server listening address.
    #[clap(long, short, default_value = "127.0.0.1")]
    address: String,
    /// The server listening port.
    #[clap(long, short, default_value_t = 9871)]
    port: u16,
}

#[tokio::main]
async fn main() {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .format_target(false)
        .format_timestamp_millis()
        .init();

    let cli = Cli::parse();
    let config = freezeout_server::Config {
        address: cli.address,
        port: cli.port,
    };

    if let Err(e) = server::run(config).await {
        error!("{e}");
    }
}
