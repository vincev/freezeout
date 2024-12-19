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
    /// Number of tables.
    #[clap(long, default_value_t = 10, value_parser = clap::value_parser!(u16).range(1..=100))]
    tables: u16,
    /// Number of seats per table.
    #[clap(long, default_value_t = 3, value_parser = clap::value_parser!(u8).range(2..=6))]
    seats: u8,
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
        tables: cli.tables as usize,
        seats: cli.seats as usize,
    };

    if let Err(e) = server::run(config).await {
        error!("{e}");
    }
}
