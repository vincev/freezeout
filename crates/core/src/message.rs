// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Type definitions for messages between the client and server.
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::{
    crypto::{PeerId, Signature, SigningKey, VerifyingKey},
    poker::{Card, Chips, PlayerCards, TableId},
};

/// Message exchanged by a client and a server.
#[derive(Debug, Serialize, Deserialize)]
pub enum Message {
    /// Joins a server with a nickname.
    JoinServer {
        /// The player nickname.
        nickname: String,
    },
    /// A player account information.
    ServerJoined {
        /// The player nickname.
        nickname: String,
        /// The chips amount for the player.
        chips: Chips,
    },
    /// Join a table.
    JoinTable,
    /// Leave a table.
    LeaveTable,
    /// Table joined confirmation.
    TableJoined {
        /// The table the player joined.
        table_id: TableId,
        /// The chips amount for the player who joined.
        chips: Chips,
        /// The number of seats at this table.
        seats: u8,
    },
    /// There are no tables left.
    NoTablesLeft,
    /// The player doesn't have enough chips to join a game.
    NotEnoughChips,
    /// The player has already joined a table.
    PlayerAlreadyJoined,
    /// A player joined the table.
    PlayerJoined {
        /// The player id.
        player_id: PeerId,
        /// The player nickname.
        nickname: String,
        /// The player chips.
        chips: Chips,
    },
    /// Show the account dialog.
    ShowAccount {
        /// The player chips.
        chips: Chips,
    },
    /// Tell players the game is starting and update the seats order.
    StartGame(Vec<PeerId>),
    /// Tell players to prepare for a new hand.
    StartHand,
    /// Tell players the hand has completed and who won.
    EndHand {
        /// List of payoffs for the hand.
        payoffs: Vec<HandPayoff>,
        /// The board cards.
        board: Vec<Card>,
        /// Players cards.
        cards: Vec<(PeerId, PlayerCards)>,
    },
    /// Deal cards to a player.
    DealCards(Card, Card),
    /// A player left the table.
    PlayerLeft(PeerId),
    /// A game state update.
    GameUpdate {
        /// The players update.
        players: Vec<PlayerUpdate>,
        /// The board cards.
        board: Vec<Card>,
        /// The pot.
        pot: Chips,
    },
    /// Request action from a player.
    ActionRequest {
        /// The player that should respond with an action.
        player_id: PeerId,
        /// The minimum raise.
        min_raise: Chips,
        /// The current big blind.
        big_blind: Chips,
        /// The list of legal actions.
        actions: Vec<PlayerAction>,
    },
    /// Player action response.
    ActionResponse {
        /// The action from the player.
        action: PlayerAction,
        /// The amount for this action (only used for bet and raise actions)
        amount: Chips,
    },
}

/// A player update details.
#[derive(Debug, Serialize, Deserialize)]
pub struct PlayerUpdate {
    /// The player id.
    pub player_id: PeerId,
    /// The player chips.
    pub chips: Chips,
    /// The player current bet.
    pub bet: Chips,
    /// The last player action.
    pub action: PlayerAction,
    /// The player action timer.
    pub action_timer: Option<u16>,
    /// The player cards.
    pub cards: PlayerCards,
    /// The player has the button.
    pub has_button: bool,
    /// The player is active in the hand.
    pub is_active: bool,
}

/// A Player action.
#[derive(Copy, Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub enum PlayerAction {
    /// No action.
    None,
    /// Player pays small blind.
    SmallBlind,
    /// Player pays big blind.
    BigBlind,
    /// Player calls.
    Call,
    /// Player checks.
    Check,
    /// Player bets.
    Bet,
    /// Player raises.
    Raise,
    /// Player folds.
    Fold,
}

impl PlayerAction {
    /// The action label.
    pub fn label(&self) -> &'static str {
        match self {
            PlayerAction::SmallBlind => "SB",
            PlayerAction::BigBlind => "BB",
            PlayerAction::Call => "CALL",
            PlayerAction::Check => "CHECK",
            PlayerAction::Bet => "BET",
            PlayerAction::Raise => "RAISE",
            PlayerAction::Fold => "FOLD",
            PlayerAction::None => "",
        }
    }
}

/// Hand payoff description.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HandPayoff {
    /// The player receiving the payment.
    pub player_id: PeerId,
    /// The payment amount.
    pub chips: Chips,
    /// The winning cards.
    pub cards: Vec<Card>,
    /// Cards rank description.
    pub rank: String,
}

/// A signed message.
#[derive(Debug, Clone)]
pub struct SignedMessage {
    /// Clonable payload for broadcasting to multiple connection tasks.
    payload: Arc<Payload>,
}

/// Private signed message payload.
#[derive(Debug, Serialize, Deserialize)]
struct Payload {
    msg: Message,
    sig: Signature,
    vk: VerifyingKey,
}

impl SignedMessage {
    /// Creates a new signed message.
    pub fn new(sk: &SigningKey, msg: Message) -> Self {
        let sig = sk.sign(&msg);
        Self {
            payload: Arc::new(Payload {
                msg,
                sig,
                vk: sk.verifying_key(),
            }),
        }
    }

    /// Deserializes this message and verifies its signature.
    pub fn deserialize_and_verify(buf: &[u8]) -> Result<Self> {
        let sm = Self {
            payload: Arc::new(bincode::deserialize::<Payload>(buf)?),
        };

        if !sm.payload.vk.verify(&sm.payload.msg, &sm.payload.sig) {
            bail!("Invalid signature");
        }

        Ok(sm)
    }

    /// Serializes this message.
    pub fn serialize(&self) -> Vec<u8> {
        bincode::serialize(self.payload.as_ref()).expect("Should serialize signed message")
    }

    /// Returns the identifier of the player who sent this message.
    pub fn sender(&self) -> PeerId {
        self.payload.vk.peer_id()
    }

    /// Extracts the signed message.
    pub fn message(&self) -> &Message {
        &self.payload.msg
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_message() {
        let sk = SigningKey::default();
        let message = Message::JoinServer {
            nickname: "Alice".to_string(),
        };

        let smsg = SignedMessage::new(&sk, message);
        let bytes = smsg.serialize();

        let deser_msg = SignedMessage::deserialize_and_verify(&bytes).unwrap();
        assert!(
            matches!(deser_msg.message(), Message::JoinServer{ nickname } if nickname == "Alice")
        );
    }
}
