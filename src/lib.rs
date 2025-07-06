#![no_std]
#![doc = "BondyBird - A Rust library for BR/EDR (Classic) Bluetooth in embedded systems"]
#![warn(missing_docs)]
#![allow(dead_code, clippy::unused_async, clippy::large_enum_variant)]

//! # BondyBird
//!
//! BondyBird is a BR/EDR (Classic) Bluetooth Host implementation for embedded devices.
//!
//! This library provides a host-side implementation of the Bluetooth Classic protocol stack,
//! designed to work with external Bluetooth controllers via HCI (Host Controller Interface).
//!
//! The implementation is built on top of Embassy's async executor and uses the `bt-hci` crate
//! for standardized HCI command and event handling.
//!
//! ## Architecture
//!
//! The simplified architecture consists of:
//!
//! 1. **Bluetooth Manager Task** - Single task that owns HCI controller and handles everything
//! 2. **Static Channels** - For communication between REST handlers and manager
//! 3. **REST Handler Functions** - Simple functions that send requests and wait for responses
//!
//! ```text
//! ┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
//! │  REST Handler   │    │  API REQUEST     │    │                 │
//! │   Functions     │───▶│    CHANNEL       │───▶│                 │
//! │                 │    └──────────────────┘    │                 │
//! └─────────────────┘                            │   Bluetooth     │
//!         ▲                                      │   Manager       │
//!         │                                      │    Task         │
//!         │              ┌──────────────────┐    │                 │
//!         └──────────────│  API RESPONSE    │◀───│                 │
//!                        │    CHANNEL       │    │                 │
//!                        └──────────────────┘    └─────────────────┘
//!                                                         │
//!                                                         │
//!                                                         ▼
//!                                                ┌──────────────────┐
//!                                                │  HCI Controller  │
//!                                                │  (select! loop)  │
//!                                                └──────────────────┘
//! ```

/// Bluetooth Manager Task - Single task that handles everything
pub mod manager;

// Export main components
pub use manager::{
    bluetooth_manager_task, handle_disconnect, handle_discover, handle_get_devices,
    handle_get_state, handle_pair,
};

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use heapless::{String, Vec};

// =============================================================================
// STATIC CHANNELS
// =============================================================================

/// Static channel for API requests from REST handlers to Bluetooth Manager
pub static API_REQUEST_CHANNEL: Channel<CriticalSectionRawMutex, ApiRequest, 8> = Channel::new();

/// Static channel for API responses from Bluetooth Manager to REST handlers
pub static API_RESPONSE_CHANNEL: Channel<CriticalSectionRawMutex, ApiResponse, 8> = Channel::new();

// =============================================================================
// CORE TYPES
// =============================================================================

/// Maximum number of simultaneous Bluetooth connections supported
pub const MAX_CONNECTIONS: usize = 4;

/// Maximum number of devices that can be discovered and stored
pub const MAX_DISCOVERED_DEVICES: usize = 8;

/// Size of the buffer used for HCI event processing
pub const EVENT_BUFFER_SIZE: usize = 255;

/// Represents a discovered Bluetooth device with its properties
#[derive(Debug, Clone, Copy)]
pub struct BluetoothDevice {
    /// 6-byte Bluetooth device address (`BD_ADDR`)
    pub addr: [u8; 6],
    /// Received Signal Strength Indicator (RSSI) in dBm, if available
    pub rssi: Option<i8>,
    /// Device name, if available (up to 32 bytes)
    pub name: Option<[u8; 32]>,
    /// Class of Device (`CoD`) indicating device type and capabilities
    pub class_of_device: Option<u32>,
}

/// Local device information collected during initialization
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalDeviceInfo {
    /// Local Bluetooth device address
    pub bd_addr: Option<[u8; 6]>,
    /// HCI version information
    pub hci_version: Option<u8>,
    /// HCI revision
    pub hci_revision: Option<u16>,
    /// LMP version
    pub lmp_version: Option<u8>,
    /// Manufacturer name
    pub manufacturer_name: Option<u16>,
    /// LMP subversion
    pub lmp_subversion: Option<u16>,
    /// Local supported features (8 bytes)
    pub local_features: Option<[u8; 8]>,
    /// ACL data packet length
    pub acl_data_packet_length: Option<u16>,
    /// SCO data packet length  
    pub sco_data_packet_length: Option<u8>,
    /// Total number of ACL data packets
    pub total_num_acl_data_packets: Option<u16>,
    /// Total number of SCO data packets
    pub total_num_sco_data_packets: Option<u16>,
}

/// Represents the current state of the Bluetooth system
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BluetoothState {
    /// Bluetooth is powered off
    PoweredOff,
    /// Bluetooth is powered on and ready
    PoweredOn,
    /// Currently discovering devices
    Discovering,
    /// Currently connecting to a device
    Connecting,
    /// Connected to a device
    Connected,
}

/// Bluetooth-related errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BluetoothError {
    /// HCI controller error
    HciError,
    /// Device not found
    DeviceNotFound,
    /// Device not connected
    DeviceNotConnected,
    /// Connection failed
    ConnectionFailed,
    /// Operation timeout
    Timeout,
    /// Operation already in progress
    AlreadyInProgress,
    /// Invalid parameter
    InvalidParameter,
    /// Not supported
    NotSupported,
    /// Discovery operation failed
    DiscoveryFailed,
}

/// API requests sent to the Bluetooth Manager
#[derive(Debug, Clone)]
pub enum ApiRequest {
    /// Start device discovery
    Discover,
    /// Stop ongoing discovery
    StopDiscovery,
    /// Get list of discovered devices
    GetDevices,
    /// Connect/pair with a device
    Pair(String<64>), // Device address as string
    /// Disconnect from a device
    Disconnect(String<64>),
    /// Get current Bluetooth state
    GetState,
}

/// API responses sent back from the Bluetooth Manager
#[derive(Debug, Clone)]
pub enum ApiResponse {
    /// Discovery completed successfully
    DiscoverComplete,
    /// Discovery stopped
    DiscoveryStopped,
    /// List of discovered devices
    Devices(Vec<BluetoothDevice, MAX_DISCOVERED_DEVICES>),
    /// Pairing completed successfully
    PairComplete,
    /// Disconnection completed
    DisconnectComplete,
    /// Current Bluetooth state
    State(BluetoothState),
    /// Error occurred
    Error(BluetoothError),
}
