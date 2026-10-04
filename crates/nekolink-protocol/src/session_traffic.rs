//! Session traffic framing: frame headers, traffic counters, and the
//! replay window that guards encrypted session control messages.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{
    validate_session_cipher, ErrorCode, ProtocolError, SESSION_AES256GCM_NONCE_LEN,
    SESSION_CIPHER_AES256GCM, SESSION_CIPHER_XCHACHA20POLY1305,
    SESSION_XCHACHA20POLY1305_NONCE_LEN,
};

pub const SESSION_REPLAY_WINDOW_SIZE: u64 = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionFrameKind {
    Control,
    File,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionFrameDirection {
    Send,
    Receive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionTrafficFrameHeader {
    pub cipher: String,
    pub kind: SessionFrameKind,
    pub direction: SessionFrameDirection,
    pub counter: u64,
    pub nonce: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SessionTrafficCounters {
    send_counter: u64,
    receive_counter: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionReplayWindow {
    window_size: u64,
    highest_counter: Option<u64>,
    seen_counters: BTreeSet<u64>,
}

impl SessionTrafficCounters {
    pub fn new(send_counter: u64, receive_counter: u64) -> Self {
        Self {
            send_counter,
            receive_counter,
        }
    }

    pub fn next_send_header(
        &mut self,
        cipher: &str,
        kind: SessionFrameKind,
    ) -> Result<SessionTrafficFrameHeader, ProtocolError> {
        let counter = next_session_counter(&mut self.send_counter)?;
        SessionTrafficFrameHeader::new(cipher, kind, SessionFrameDirection::Send, counter)
    }

    pub fn next_receive_header(
        &mut self,
        cipher: &str,
        kind: SessionFrameKind,
    ) -> Result<SessionTrafficFrameHeader, ProtocolError> {
        let counter = next_session_counter(&mut self.receive_counter)?;
        SessionTrafficFrameHeader::new(cipher, kind, SessionFrameDirection::Receive, counter)
    }
}

impl Default for SessionReplayWindow {
    fn default() -> Self {
        Self {
            window_size: SESSION_REPLAY_WINDOW_SIZE,
            highest_counter: None,
            seen_counters: BTreeSet::new(),
        }
    }
}

impl SessionReplayWindow {
    pub fn with_window_size(window_size: u64) -> Result<Self, ProtocolError> {
        if window_size == 0 {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                "session replay window size cannot be zero",
            ));
        }

        Ok(Self {
            window_size,
            highest_counter: None,
            seen_counters: BTreeSet::new(),
        })
    }

    pub fn accept(&mut self, header: &SessionTrafficFrameHeader) -> Result<(), ProtocolError> {
        if let Some(highest_counter) = self.highest_counter {
            let minimum_counter =
                highest_counter.saturating_sub(self.window_size.saturating_sub(1));
            if header.counter < minimum_counter {
                return Err(ProtocolError::new(
                    ErrorCode::InvalidPayload,
                    "session frame counter is outside replay window",
                ));
            }
        }

        if self.seen_counters.contains(&header.counter) {
            return Err(ProtocolError::new(
                ErrorCode::InvalidPayload,
                "replayed session frame counter",
            ));
        }

        self.seen_counters.insert(header.counter);
        self.highest_counter = Some(
            self.highest_counter
                .map_or(header.counter, |highest| highest.max(header.counter)),
        );
        self.prune();

        Ok(())
    }

    fn prune(&mut self) {
        let Some(highest_counter) = self.highest_counter else {
            return;
        };
        let minimum_counter = highest_counter.saturating_sub(self.window_size.saturating_sub(1));
        self.seen_counters
            .retain(|counter| *counter >= minimum_counter);
    }
}

impl SessionTrafficFrameHeader {
    pub fn new(
        cipher: &str,
        kind: SessionFrameKind,
        direction: SessionFrameDirection,
        counter: u64,
    ) -> Result<Self, ProtocolError> {
        validate_session_cipher("cipher", cipher)?;
        Ok(Self {
            cipher: cipher.to_string(),
            kind,
            direction,
            counter,
            nonce: session_frame_nonce(cipher, counter)?,
        })
    }
}

impl SessionFrameDirection {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Send => "send",
            Self::Receive => "receive",
        }
    }
}

fn next_session_counter(counter: &mut u64) -> Result<u64, ProtocolError> {
    if *counter == u64::MAX {
        return Err(ProtocolError::new(
            ErrorCode::InvalidPayload,
            "session traffic counter exhausted",
        ));
    }
    let current = *counter;
    *counter += 1;
    Ok(current)
}

fn session_frame_nonce(cipher: &str, counter: u64) -> Result<Vec<u8>, ProtocolError> {
    validate_session_cipher("cipher", cipher)?;
    let nonce_len = match cipher {
        SESSION_CIPHER_XCHACHA20POLY1305 => SESSION_XCHACHA20POLY1305_NONCE_LEN,
        SESSION_CIPHER_AES256GCM => SESSION_AES256GCM_NONCE_LEN,
        _ => unreachable!("validate_session_cipher rejects unsupported ciphers"),
    };
    let mut nonce = vec![0_u8; nonce_len];
    let counter_offset = nonce_len - std::mem::size_of::<u64>();
    nonce[counter_offset..].copy_from_slice(&counter.to_be_bytes());
    Ok(nonce)
}
