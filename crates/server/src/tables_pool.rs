// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Tables pool.
use std::sync::Arc;
use tokio::sync::{Mutex, broadcast, mpsc};

use freezeout_core::{
    crypto::{PeerId, SigningKey},
    poker::Chips,
};

use crate::{
    db::Db,
    table::{Table, TableMessage},
};

/// A pool of tables players can join.
#[derive(Debug, Clone)]
pub struct TablesPool(Arc<Mutex<Shared>>);

#[derive(Debug)]
struct Shared {
    tables: Vec<Arc<Table>>,
}

impl TablesPool {
    /// Creates a new table pool.
    pub fn new(
        tables: usize,
        seats: usize,
        sk: Arc<SigningKey>,
        db: Db,
        shutdown_broadcast_tx: &broadcast::Sender<()>,
        shutdown_complete_tx: &mpsc::Sender<()>,
    ) -> Self {
        let tables = (0..tables)
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

        let state = Shared { tables };

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

        for idx in 0..pool.tables.len() {
            // Try to join a table, a join may fail if the player is already at the
            // table or if a game is in progress.
            let res = pool.tables[idx]
                .try_join(player_id, nickname, join_chips, table_tx.clone())
                .await;
            if res.is_ok() {
                let table = pool.tables[idx].clone();
                if table.has_game_started().await {
                    // After a successful join if the game started move this table at
                    // the back of the queue so that we look at free tables first.
                    let table = pool.tables.remove(idx);
                    pool.tables.push(table);
                }

                return Some(table);
            }
        }

        // All tables are busy.
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use freezeout_core::poker::TableId;

    struct TestPool {
        pool: TablesPool,
        _shutdown_broadcast_tx: broadcast::Sender<()>,
        _shutdown_complete_rx: mpsc::Receiver<()>,
    }

    impl TestPool {
        fn new(n: usize) -> Self {
            let sk = SigningKey::default();
            let db = Db::open_in_memory().unwrap();
            let (shutdown_complete_tx, shutdown_complete_rx) = mpsc::channel(1);
            let (shutdown_broadcast_tx, _) = broadcast::channel(1);
            let pool = TablesPool::new(
                n,
                2,
                Arc::new(sk),
                db,
                &shutdown_broadcast_tx,
                &shutdown_complete_tx,
            );

            Self {
                pool,
                _shutdown_broadcast_tx: shutdown_broadcast_tx,
                _shutdown_complete_rx: shutdown_complete_rx,
            }
        }

        async fn join(&self, p: &TestPlayer) -> Option<Arc<Table>> {
            self.pool
                .join(&p.peer_id, "nn", Chips::new(1_000_000), p.tx.clone())
                .await
        }

        async fn table_ids(&self) -> Vec<TableId> {
            let pool = self.pool.0.lock().await;
            pool.tables.iter().map(|t| t.table_id()).collect()
        }
    }

    struct TestPlayer {
        tx: mpsc::Sender<TableMessage>,
        _rx: mpsc::Receiver<TableMessage>,
        peer_id: PeerId,
    }

    impl TestPlayer {
        fn new() -> Self {
            let sk = SigningKey::default();
            let peer_id = sk.verifying_key().peer_id();
            let (tx, rx) = mpsc::channel(64);
            Self {
                tx,
                _rx: rx,
                peer_id,
            }
        }
    }

    #[tokio::test]
    async fn test_table_pool() {
        let tp = TestPool::new(2);
        let tids = tp.table_ids().await;

        // Player 1 join table 1 that should be in first position.
        let p1 = TestPlayer::new();
        let t1 = tp.join(&p1).await.unwrap();
        assert_eq!(t1.table_id(), tids[0]);

        // Player 2 join table 1.
        let p2 = TestPlayer::new();
        let t1 = tp.join(&p2).await.unwrap();
        assert_eq!(t1.table_id(), tids[0]);

        // As the table is full it should move at the back of the queue.
        let tids = tp.table_ids().await;
        assert_eq!(t1.table_id(), tids[1]);

        // Player 1 join table 2, table 2 should be at front of the queue.
        let t2 = tp.join(&p1).await.unwrap();
        assert_eq!(t2.table_id(), tids[0]);

        // Player 2 join table 2.
        let t2 = tp.join(&p2).await.unwrap();
        assert_eq!(t2.table_id(), tids[0]);

        // Player 3 tries to join but there are no tables.
        let p3 = TestPlayer::new();
        assert!(tp.join(&p3).await.is_none());

        // Players 2 leaves table 1 that becomes ready because with one player left
        // the game ends (2 seats per table), table 1 should move to the front.
        t1.leave(&p2.peer_id).await;

        let tids = tp.table_ids().await;
        assert_eq!(t1.table_id(), tids[0]);

        // Player 1 join table 2.
        let t2 = tp.join(&p1).await.unwrap();
        assert_eq!(t2.table_id(), tids[0]);

        // Player 2 join table 2.
        let t2 = tp.join(&p2).await.unwrap();
        assert_eq!(t2.table_id(), tids[0]);
    }
}
