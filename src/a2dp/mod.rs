//! A2DP (Advanced Audio Distribution Profile) Implementation
//!
//! This module implements the A2DP profile for streaming audio over Bluetooth.
//! A2DP uses AVDTP (Audio/Video Distribution Transport Protocol) over L2CAP
//! to establish audio streaming sessions between source and sink devices.
//!
//! ## Architecture
//!
//! - **A2DP Profile**: High-level audio streaming profile
//! - **AVDTP Layer**: Transport protocol for media streams
//! - **L2CAP Integration**: Uses PSM 0x0019 for AVDTP signaling
//! - **Codec Support**: SBC (Sub-band Coding) codec implementation
//!
//! ## Usage
//!
//! ```rust
//! use bondybird::a2dp::{A2dpProfile, Role};
//! use bondybird::a2dp::codec::SbcCapabilities;
//!
//! let mut profile = A2dpProfile::new(Role::Source);
//! let _seid = profile.add_sbc_endpoint(SbcCapabilities::default()).unwrap();
//! ```

pub mod avdtp;
pub mod codec;
pub mod profile;

pub use avdtp::*;
pub use codec::*;
pub use profile::*;

use crate::l2cap::packet::ProtocolServiceMultiplexer;

/// A2DP uses AVDTP protocol over L2CAP PSM 0x0019
pub const A2DP_PSM: ProtocolServiceMultiplexer = 0x0019;

/// Stream Endpoint Identifier (SEID) type
pub type StreamEndpointId = u8;

/// Stream Handle for active streams
pub type StreamHandle = u16;

/// A2DP Profile Roles
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Audio source (sends audio)
    Source,
    /// Audio sink (receives audio)
    Sink,
}

/// A2DP Connection State
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// No connection
    Disconnected,
    /// L2CAP connection established, AVDTP signaling ready
    Connected,
    /// Stream endpoint discovered and configured
    Configured,
    /// Audio stream is active
    Streaming,
    /// Stream is suspended
    Suspended,
}

/// A2DP Errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum A2dpError {
    /// Invalid stream endpoint
    InvalidEndpoint,
    /// Codec not supported
    UnsupportedCodec,
    /// Stream configuration failed
    ConfigurationFailed,
    /// Stream not ready for operation
    StreamNotReady,
    /// L2CAP error
    L2capError,
    /// AVDTP protocol error
    AvdtpError,
}
