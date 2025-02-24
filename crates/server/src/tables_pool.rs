// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Freezeout Poker server entry point.
use ahash::AHashMap;
use std::{collections::VecDeque, sync::Arc};
use tokio::sync::{Mutex, broadcast, mpsc};

use freezeout_core::{
    crypto::{PeerId, SigningKey},
    poker::{Chips, TableId},
};

use crate::{
    db::Db,
    table::{Table, TableMessage},
};

/// The tables on this server.
#[derive(Debug, Clone)]
pub struct TablesPool(Arc<Mutex<Shared>>);

#[derive(Debug)]
struct Shared {
    /// Tables that are waiting for players.
    waiting: VecDeque<Arc<Table>>,
    /// Tables with running games.
    playing: AHashMap<TableId, Arc<Table>>,
}

impl TablesPool {
    /// Creates a new table set.
    pub fn new(
        tables: usize,
        seats: usize,
        sk: Arc<SigningKey>,
        db: Db,
        shutdown_broadcast_tx: &broadcast::Sender<()>,
        shutdown_complete_tx: &mpsc::Sender<()>,
    ) -> Self {
        let waiting = (0..tables)
            .map(|_| {
                Arc::new(Table::new(
                    seats,
                    sk.clone(),
                    db.clone(),
                    shutdown_broadcast_tx.subscribe(),
                    shutdown_complete_tx.clone(),
                ))
            })
            .collect();

        let state = Shared {
            waiting,
            playing: Default::default(),
        };

        Self(Arc::new(Mutex::new(state)))
    }

    /// Try to join a table in the pool.
    pub async fn join(
        &self,
        player_id: &PeerId,
        nickname: &str,
        join_chips: Chips,
        table_tx: mpsc::Sender<TableMessage>,
    ) -> Option<Arc<Table>> {
        let mut pool = self.0.lock().await;

        // Move full tables to playing set.
        while let Some(table) = pool.waiting.front() {
            if table.has_game_started().await {
                let table = pool.waiting.pop_front().unwrap();
                pool.playing.insert(table.table_id(), table);
            } else {
                break;
            }
        }

        // No waiting tables.
        if pool.waiting.is_empty() {
            return None;
        }

        // Try to join a table.
        for table in &pool.waiting {
            if table
                .join(player_id, nickname, join_chips, table_tx.clone())
                .await
                .is_ok()
            {
                return Some(table.clone());
            }
        }

        None
    }

    /// Release the table back to the waiting pool.
    pub async fn release(&self, table_id: TableId) {
        let mut pool = self.0.lock().await;
        if let Some(table) = pool.playing.remove(&table_id) {
            pool.waiting.push_back(table);
        }
    }
}
