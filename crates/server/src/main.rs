// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0
use clap::Parser;
use freezeout_server::server;
use log::error;
use std::path::PathBuf;

#[derive(Debug, Parser)]
struct Cli {
    /// The server listening address.
    #[arg(long, short, default_value = "127.0.0.1")]
    address: String,
    /// The server listening port.
    #[arg(long, short, default_value_t = 9871)]
    port: u16,
    /// Number of tables.
    #[arg(long, default_value_t = 10, value_parser = clap::value_parser!(u16).range(1..=1_000))]
    tables: u16,
    /// Number of seats per table.
    #[arg(long, default_value_t = 3, value_parser = clap::value_parser!(u8).range(2..=6))]
    seats: u8,
    /// Application data path.
    #[arg(long)]
    data_path: Option<PathBuf>,
    /// TLS private key PEM path.
    #[arg(long, requires = "chain_path")]
    key_path: Option<PathBuf>,
    /// TLS certificate chain PEM path.
    #[arg(long, requires = "key_path")]
    chain_path: Option<PathBuf>,
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
        data_path: cli.data_path,
        key_path: cli.key_path,
        chain_path: cli.chain_path,
    };

    if let Err(e) = server::run(config).await {
        error!("{e}");
    }
}
