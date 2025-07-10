//! `BondyBird` Constants
//!
//! This module contains all the constants used throughout the `BondyBird` library.
//! These constants define various limits, default values, and Bluetooth-specific
//! parameters used in the implementation.

/// Maximum number of simultaneous ACL connections
pub const MAX_CHANNELS: usize = 8;

/// General Inquiry Access Code (GIAC) - standard inquiry LAP
pub const GIAC: [u8; 3] = [0x33, 0x8B, 0x9E];

/// Default inquiry duration in 1.28s units (0x30 = ~61 seconds)
pub const DEFAULT_INQUIRY_DURATION: u8 = 0x30;

/// Unlimited number of inquiry responses
pub const UNLIMITED_RESPONSES: u8 = 0;

/// Reserved field value
pub const RESERVED_FIELD: u8 = 0x00;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_constants() {
        assert_eq!(MAX_CHANNELS, 8);
    }

    #[test]
    fn test_giac_constant() {
        assert_eq!(GIAC.len(), 3);
        assert_eq!(GIAC, [0x33, 0x8B, 0x9E]);
    }

    #[test]
    fn test_inquiry_constants() {
        assert_eq!(DEFAULT_INQUIRY_DURATION, 0x30);
        assert_eq!(UNLIMITED_RESPONSES, 0);
    }

    #[test]
    fn test_device_constants() {
        assert_eq!(MAX_DEVICE_NAME_LENGTH, 32);
        assert_eq!(BD_ADDR_LENGTH, 6);
        assert_eq!(CLASS_OF_DEVICE_LENGTH, 3);
    }

    #[test]
    fn test_connection_constants() {
        assert_eq!(MAX_CONNECTIONS, 4);
        assert_eq!(MAX_DISCOVERED_DEVICES, 8);
        assert_eq!(MAX_CLEANUP_ENTRIES, 8);
    }

    #[test]
    fn test_buffer_constants() {
        assert_eq!(EVENT_BUFFER_SIZE, 255);
    }

    #[test]
    fn test_constants_consistency() {
        // Test that constants make sense in relation to each other
    }

    #[test]
    fn test_bluetooth_standard_compliance() {
        // Test that our constants comply with Bluetooth standards
        assert_eq!(BD_ADDR_LENGTH, 6); // Bluetooth BD_ADDR is always 6 bytes
        assert_eq!(CLASS_OF_DEVICE_LENGTH, 3); // CoD is always 3 bytes
        assert_eq!(EVENT_BUFFER_SIZE, 255); // Max HCI event size

        // GIAC should be the standard General Inquiry Access Code
        assert_eq!(GIAC, [0x33, 0x8B, 0x9E]);
    }
}
