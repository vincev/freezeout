// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Database types for persisting state.
use anyhow::Result;
use parking_lot::Mutex;
use rusqlite::{params, Connection};
use std::{path::Path, sync::Arc};

use freezeout_core::{crypto::PeerId, poker::Chips};

/// A database player row.
#[derive(Debug)]
pub struct Player {
    /// The player id.
    pub player_id: PeerId,
    /// The player chips.
    pub chips: Chips,
}

/// Database for persisting game and players state.
#[derive(Debug, Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

impl Db {
    /// Open a database.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let conn = Connection::open(path)?;

        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;

        // Create tables
        conn.execute(
            "CREATE TABLE IF NOT EXISTS players (
               id TEXT PRIMARY KEY,
               chips INTEGER NOT NULL,
               created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
               last_update DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            (),
        )?;

        Ok(Db {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Updates a playe state.
    pub async fn update(&self, players: Vec<Player>) -> Result<()> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = conn.lock();

            let tx = conn.transaction()?;

            for player in players {
                tx.execute(
                    "UPDATE players SET
                       chips = ?1,
                       last_update = CURRENT_TIMESTAMP
                     WHERE id = ?2",
                    params![player.chips.amount(), player.player_id.digits()],
                )?;
            }

            tx.commit()?;

            Ok(())
        })
        .await?
    }

    /// Get a player or insert one with the given number of chips.
    pub async fn get_or_insert_player(&self, player_id: PeerId, chips: Chips) -> Result<Player> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock();

            let mut stmt = conn.prepare(
                "SELECT id, chips
                 FROM players
                 WHERE id = ?1",
            )?;

            let res = stmt.query_row(params![player_id.digits()], |row| {
                Ok(Player {
                    player_id: player_id.clone(),
                    chips: Chips::from(row.get::<usize, i32>(1)? as u32),
                })
            });

            match res {
                Ok(player) => Ok(player),
                Err(rusqlite::Error::QueryReturnedNoRows) => {
                    let player = Player { player_id, chips };

                    conn.execute(
                        "INSERT INTO players (id, chips, last_update)
                         VALUES (?1, ?2, CURRENT_TIMESTAMP)",
                        params![player.player_id.digits(), player.chips.amount()],
                    )?;

                    Ok(player)
                }
                Err(e) => Err(e.into()),
            }
        })
        .await?
    }
}
