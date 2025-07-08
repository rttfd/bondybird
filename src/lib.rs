#![no_std]
#![doc = include_str!("../README.md")]
#![warn(missing_docs)]
#![allow(
    dead_code,
    clippy::unused_async,
    clippy::large_enum_variant,
    clippy::too_many_lines
)]

mod api;
pub mod constants;
mod host;
mod processor;

use constants::{MAX_CHANNELS, MAX_DISCOVERED_DEVICES};
use embassy_sync::channel::Channel;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};
use heapless::{FnvIndexMap, String, Vec};

pub use api::{connect_device, disconnect_device, get_devices, get_state, start_discovery};
pub use processor::{api_request_processor, hci_event_processor};

pub(crate) static REQUEST_CHANNEL: Channel<CriticalSectionRawMutex, Request, MAX_CHANNELS> =
    Channel::new();

pub(crate) static RESPONSE_CHANNEL: Channel<CriticalSectionRawMutex, Response, MAX_CHANNELS> =
    Channel::new();

pub(crate) static BLUETOOTH_HOST: Mutex<CriticalSectionRawMutex, BluetoothHost> =
    Mutex::new(BluetoothHost {
        state: BluetoothState::PoweredOff,
        devices: FnvIndexMap::new(),
        connections: FnvIndexMap::new(),
        local_info: LocalDeviceInfo {
            bd_addr: None,
            hci_version: None,
            hci_revision: None,
            lmp_version: None,
            manufacturer_name: None,
            lmp_subversion: None,
            local_features: None,
            acl_data_packet_length: None,
            sco_data_packet_length: None,
            total_num_acl_data_packets: None,
            total_num_sco_data_packets: None,
        },
        discovering: false,
    });

/// A Bluetooth Device Address (`BD_ADDR`) wrapper for type safety
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct BluetoothAddress(pub [u8; 6]);

impl BluetoothAddress {
    /// Create a new Bluetooth address from bytes
    #[must_use]
    pub const fn new(addr: [u8; 6]) -> Self {
        Self(addr)
    }

    /// Get the raw address bytes
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 6] {
        &self.0
    }

    /// Format the address as a colon-separated hex string
    #[must_use]
    pub fn format_hex(&self) -> heapless::String<17> {
        let mut result = heapless::String::new();
        for (i, byte) in self.0.iter().enumerate() {
            if i > 0 {
                result.push(':').ok();
            }
            // Format byte as hex
            let hex_chars = [
                '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'A', 'B', 'C', 'D', 'E', 'F',
            ];
            result.push(hex_chars[(byte >> 4) as usize]).ok();
            result.push(hex_chars[(byte & 0x0F) as usize]).ok();
        }
        result
    }
}

impl From<[u8; 6]> for BluetoothAddress {
    fn from(addr: [u8; 6]) -> Self {
        Self(addr)
    }
}

impl From<BluetoothAddress> for [u8; 6] {
    fn from(addr: BluetoothAddress) -> Self {
        addr.0
    }
}

/// Represents a discovered Bluetooth device with its properties
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct BluetoothDevice {
    /// Bluetooth device address (`BD_ADDR`)
    pub addr: BluetoothAddress,
    /// Received Signal Strength Indicator (RSSI) in dBm, if available
    pub rssi: Option<i8>,
    /// Device name, if available (up to 32 bytes)
    pub name: Option<[u8; 32]>,
    /// Class of Device (`CoD`) indicating device type and capabilities
    pub class_of_device: Option<u32>,
}

impl BluetoothDevice {
    /// Create a new Bluetooth device
    #[must_use]
    pub fn new(addr: BluetoothAddress) -> Self {
        Self {
            addr,
            rssi: None,
            name: None,
            class_of_device: None,
        }
    }

    /// Update device with new RSSI information
    #[must_use]
    pub fn with_rssi(mut self, rssi: i8) -> Self {
        self.rssi = Some(rssi);
        self
    }

    /// Update device with new name information
    #[must_use]
    pub fn with_name(mut self, name: [u8; 32]) -> Self {
        self.name = Some(name);
        self
    }

    /// Update device with class of device information
    #[must_use]
    pub fn with_class_of_device(mut self, class_of_device: u32) -> Self {
        self.class_of_device = Some(class_of_device);
        self
    }
}

/// Local device information collected during initialization
#[derive(Debug, Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
pub struct LocalDeviceInfo {
    /// Local Bluetooth device address
    pub bd_addr: Option<BluetoothAddress>,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, defmt::Format)]
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

/// Bluetooth-related errors with detailed error information
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BluetoothError {
    /// HCI controller communication error
    HciError,
    /// Device with specified address not found in discovered devices
    DeviceNotFound,
    /// Device is not currently connected
    DeviceNotConnected,
    /// Failed to establish connection to device
    ConnectionFailed,
    /// Operation timed out
    Timeout,
    /// Another operation of the same type is already in progress
    AlreadyInProgress,
    /// Invalid parameter provided (e.g., malformed address)
    InvalidParameter,
    /// Operation not supported by the current implementation
    NotSupported,
    /// Device discovery operation failed
    DiscoveryFailed,
    /// Controller initialization failed
    InitializationFailed,
    /// Transport layer error
    TransportError,
    /// Invalid device state for the requested operation
    InvalidState,
}

/// Shared Bluetooth data structure containing state, devices, and connections
#[derive(Debug)]
pub struct BluetoothHost {
    /// Current Bluetooth state
    state: BluetoothState,
    /// Discovered devices
    devices: FnvIndexMap<BluetoothAddress, BluetoothDevice, MAX_DISCOVERED_DEVICES>,
    /// Connection handles for connected devices (`BD_ADDR` -> `ConnHandle`)
    connections: FnvIndexMap<BluetoothAddress, u16, MAX_DISCOVERED_DEVICES>,
    /// Local device information
    local_info: LocalDeviceInfo,
    /// Current discovery state
    discovering: bool,
}

/// API requests sent to the Bluetooth processing tasks
#[derive(Debug, Clone)]
pub enum Request {
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

/// API responses sent back from the Bluetooth processing tasks
#[derive(Debug, Clone)]
pub enum Response {
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
