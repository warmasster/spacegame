//! What the clients send, once it has come out of their channel: what is for the game is kept
//! until the game takes it; pings are answered and chat lines passed on at once.
use super::{GameIn, Server, ServerEvent, pack};
use crate::clock::micros;
use crate::proto::Msg;
use crate::text;

/// What clients said to the game and it has not taken yet, at most (a game that stalls must not
/// make the server keep everything a flood of them says).
const MAX_GAME_IN: usize = 1 << 14;

impl Server {
    pub(super) fn message(&mut self, i: usize, reliable: bool, bytes: &[u8], now: f64) {
        let msg = match Msg::decode(bytes) {
            // A reliable kind that came unreliably (or the reverse) is not something our client sends.
            Ok(m) if m.reliable() == reliable => m,
            _ => {
                self.stats.garbled += 1;
                return;
            }
        };
        let id = self.sessions[i].id;
        match msg {
            Msg::Ping { t } => {
                let pong = pack(&Msg::Pong { t, server: micros(now) }, 32, &mut self.msg);
                self.sessions[i].channel.send_unreliable(pong);
            }
            // (for the game that runs in the server: kept until it takes it)
            Msg::Game(data) | Msg::Quick(data) => {
                if self.game_in.len() < MAX_GAME_IN {
                    self.game_in.push(GameIn { from: id, reliable, data: data.to_vec() });
                }
            }
            Msg::Chat { text } => {
                let text = text::clean(text, text::CHAT_CHARS);
                if !text.is_empty() {
                    // To the one who said it too: everyone reads the lines in the same order.
                    self.send_all(&Msg::Said { from: Some(id), text: &text }, None);
                    self.events.push(ServerEvent::Chat { id, name: self.sessions[i].name.clone(), text });
                }
            }
            _ => self.stats.garbled += 1,
        }
    }
}
