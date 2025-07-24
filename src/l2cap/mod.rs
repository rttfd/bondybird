//! L2CAP (Logical Link Control and Adaptation Protocol) Implementation
//!
//! This module provides L2CAP protocol support for Bluetooth audio streaming,
//! implementing connection-oriented channels, signaling, and data transfer
//! as defined in the Bluetooth Core Specification.

pub mod channel;
pub mod packet;
pub mod processor;
pub mod signaling;

// TODO: Uncomment these modules as they are implemented
// pub mod config;
