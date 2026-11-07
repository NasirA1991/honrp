//! Pure derivation of the Discord/Rich Presence status shown in the UI.
//!
//! Kept separate from egui so the (occasionally surprising) combinations can
//! be table-tested without a display or a running daemon.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiscordState {
    /// The daemon is not running.
    Off,
    /// The daemon did not report whether Discord is reachable (older build).
    Unknown,
    /// No Discord IPC endpoint was found.
    NotRunning,
    /// Discord is reachable; nothing is being published because no game runs.
    Ready,
    /// A valid Heroes of Newerth card is being published.
    Live,
    /// Discord is reachable but the connection failed while a game is running.
    Unavailable,
}

/// `found`: `Some(true)` = IPC endpoint exists, `Some(false)` = none found,
/// `None` = the daemon did not report it.
pub fn derive(daemon: bool, connected: bool, found: Option<bool>, game_running: bool) -> DiscordState {
    if !daemon {
        return DiscordState::Off;
    }
    if connected {
        return DiscordState::Live;
    }
    match found {
        None => DiscordState::Unknown,
        Some(false) => DiscordState::NotRunning,
        Some(true) if game_running => DiscordState::Unavailable,
        Some(true) => DiscordState::Ready,
    }
}

/// True for states where the user can do something about it.
pub fn needs_attention(state: DiscordState, game_running: bool) -> bool {
    match state {
        DiscordState::Unknown => true,
        DiscordState::NotRunning => game_running,
        DiscordState::Unavailable => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table() {
        use DiscordState::*;
        // daemon, connected, found, game_running -> state
        let cases = [
            (false, false, None, false, Off),
            (false, false, Some(true), true, Off),
            (true, false, None, false, Unknown),
            (true, false, None, true, Unknown),
            (true, false, Some(false), false, NotRunning),
            (true, false, Some(false), true, NotRunning),
            (true, false, Some(true), false, Ready),
            (true, false, Some(true), true, Unavailable),
            (true, true, Some(true), true, Live),
            (true, true, None, true, Live),
            (true, true, Some(false), false, Live),
        ];
        for (daemon, connected, found, game, expected) in cases {
            assert_eq!(
                derive(daemon, connected, found, game),
                expected,
                "daemon={daemon} connected={connected} found={found:?} game={game}"
            );
        }
    }

    #[test]
    fn attention_states() {
        use DiscordState::*;
        assert!(!needs_attention(Off, true));
        assert!(needs_attention(Unknown, false));
        assert!(!needs_attention(NotRunning, false));
        assert!(needs_attention(NotRunning, true));
        assert!(needs_attention(Unavailable, true));
        assert!(!needs_attention(Ready, false));
        assert!(!needs_attention(Live, true));
    }
}
