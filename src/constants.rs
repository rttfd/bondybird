//! `BondyBird` Constants
//!
//! This module contains all the constants used throughout the `BondyBird` library.
//! These constants define various limits, default values, and Bluetooth-specific
//! parameters used in the implementation.

/// Maximum number of simultaneous ACL connections
pub const MAX_CHANNELS: usize = 8;

/// General Inquiry Access Code (GIAC) - standard inquiry LAP
pub const GIAC: [u8; 3] = [0x9E, 0x8B, 0x33];

/// Default inquiry duration in 1.28s units (0x30 = ~61 seconds)
pub const DEFAULT_INQUIRY_DURATION: u8 = 0x30;

/// Unlimited number of inquiry responses
pub const UNLIMITED_RESPONSES: u8 = 0;

/// Standard packet types for ACL connections (DM1, DM3, DM5, DH1, DH3, DH5)
pub const DEFAULT_PACKET_TYPES: u16 = 0xCC18;

/// Page scan repetition mode R1
pub const PAGE_SCAN_REPETITION_MODE_R1: u8 = 0x01;

/// Reserved field value
pub const RESERVED_FIELD: u8 = 0x00;

/// No clock offset specified
pub const NO_CLOCK_OFFSET: u16 = 0x0000;

/// Allow role switch during connection
pub const ALLOW_ROLE_SWITCH: u8 = 0x01;

/// Maximum number of connection cleanup entries
pub const MAX_CLEANUP_ENTRIES: usize = 8;

/// Maximum device name length in bytes
pub const MAX_DEVICE_NAME_LENGTH: usize = 32;

/// `BD_ADDR` length in bytes
pub const BD_ADDR_LENGTH: usize = 6;

/// Class of Device length in bytes
pub const CLASS_OF_DEVICE_LENGTH: usize = 3;

/// Maximum number of simultaneous Bluetooth connections supported
pub const MAX_CONNECTIONS: usize = 4;

/// Maximum number of devices that can be discovered and stored
pub const MAX_DISCOVERED_DEVICES: usize = 8;

/// Size of the buffer used for HCI event processing
pub const EVENT_BUFFER_SIZE: usize = 255;
