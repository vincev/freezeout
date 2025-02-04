// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Database types for persisting state.
use anyhow::{bail, Result};
use parking_lot::Mutex;
use rusqlite::{params, Connection};
use std::{path::Path, sync::Arc};

use freezeout_core::{crypto::PeerId, poker::Chips};

/// A database player row.
#[derive(Debug)]
pub struct Player {
    /// The player id.
    pub player_id: PeerId,
    /// The player nickname.
    pub nickname: String,
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
               nickname TEXT NOT NULL,
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

    /// A player join the server.
    pub async fn join_server(
        &self,
        player_id: PeerId,
        nickname: &str,
        join_chips: Chips,
    ) -> Result<Player> {
        let conn = self.conn.clone();
        let nickname = nickname.to_string();

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock();

            let mut stmt = conn.prepare(
                "SELECT id, nickname, chips
                 FROM players
                 WHERE id = ?1",
            )?;

            let res = stmt.query_row(params![player_id.digits()], |row| {
                Ok(Player {
                    player_id: player_id.clone(),
                    nickname: row.get(1)?,
                    chips: Chips::from(row.get::<usize, i32>(2)? as u32),
                })
            });

            match res {
                Ok(mut player) => {
                    let mut do_update = false;

                    // Reset player chips if less than join chips.
                    if player.chips < join_chips {
                        player.chips = join_chips;
                        do_update = true;
                    }

                    // Update nickname if the player joined with a different one.
                    if player.nickname != nickname {
                        player.nickname = nickname.to_string();
                        do_update = true;
                    }

                    if do_update {
                        conn.execute(
                            "UPDATE players SET
                                chips = ?2,
                                nickname = ?3,
                                last_update = CURRENT_TIMESTAMP
                             WHERE id = ?1",
                            params![
                                player.player_id.digits(),
                                player.chips.amount(),
                                player.nickname
                            ],
                        )?;
                    }

                    Ok(player)
                }
                Err(rusqlite::Error::QueryReturnedNoRows) => {
                    // If this is a new player add it to the database.
                    let player = Player {
                        player_id,
                        nickname: nickname.to_string(),
                        chips: join_chips,
                    };

                    conn.execute(
                        "INSERT INTO players (id, nickname, chips, last_update)
                         VALUES (?1, ?2, ?3, CURRENT_TIMESTAMP)",
                        params![player.player_id.digits(), nickname, player.chips.amount()],
                    )?;

                    Ok(player)
                }
                Err(e) => Err(e.into()),
            }
        })
        .await?
    }

    /// Pay an amount of chips from a player.
    ///
    /// Returns Ok(false) if the player doesn't have enough chips or an error if the
    /// player cannot be found.
    pub async fn pay_from_player(&self, player_id: PeerId, amount: Chips) -> Result<bool> {
        let conn = self.conn.clone();

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock();

            let mut stmt = conn.prepare("SELECT chips FROM players WHERE id = ?1")?;
            let res = stmt.query_row(params![player_id.digits()], |row| {
                Ok(Chips::from(row.get::<usize, i32>(0)? as u32))
            });

            match res {
                Ok(chips) => {
                    if chips < amount {
                        // Not enough chips.
                        return Ok(false);
                    }

                    // Update chips for this player.
                    conn.execute(
                        "UPDATE players SET
                           chips = chips - ?2,
                           last_update = CURRENT_TIMESTAMP
                         WHERE id = ?1",
                        params![player_id.digits(), amount.amount(),],
                    )?;

                    Ok(true)
                }
                Err(e) => Err(e.into()),
            }
        })
        .await?
    }

    /// Pay an amount of chips to a player.
    ///
    /// Returns an error if the player has not been found.
    pub async fn pay_to_player(&self, player_id: PeerId, amount: Chips) -> Result<()> {
        let conn = self.conn.clone();

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock();

            let num_rows = conn.execute(
                "UPDATE players SET
                   chips = chips + ?2,
                   last_update = CURRENT_TIMESTAMP
                 WHERE id = ?1",
                params![player_id.digits(), amount.amount(),],
            )?;

            if num_rows == 0 {
                bail!("Player {player_id} not found");
            } else {
                Ok(())
            }
        })
        .await?
    }

    /// Returns the player with the given id.
    pub async fn get_player(&self, player_id: PeerId) -> Result<Player> {
        let conn = self.conn.clone();

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock();

            let mut stmt = conn.prepare(
                "SELECT id, nickname, chips
                 FROM players
                 WHERE id = ?1",
            )?;

            stmt.query_row(params![player_id.digits()], |row| {
                Ok(Player {
                    player_id: player_id.clone(),
                    nickname: row.get(1)?,
                    chips: Chips::from(row.get::<usize, i32>(2)? as u32),
                })
            })
            .map_err(anyhow::Error::from)
        })
        .await?
    }
}
