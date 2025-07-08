#![no_std]
#![doc = include_str!("../README.md")]
#![warn(missing_docs)]
#![allow(
    dead_code,
    clippy::unused_async,
    clippy::large_enum_variant,
    clippy::too_many_lines
)]

pub mod api;
pub mod constants;
pub mod host;
pub mod processor;

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

impl From<BluetoothAddress> for bt_hci::param::BdAddr {
    fn from(addr: BluetoothAddress) -> Self {
        bt_hci::param::BdAddr::new(addr.0)
    }
}

impl TryFrom<&[u8]> for BluetoothAddress {
    type Error = BluetoothError;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        if bytes.len() == 6 {
            let mut addr = [0u8; 6];
            addr.copy_from_slice(bytes);
            Ok(BluetoothAddress(addr))
        } else {
            Err(BluetoothError::InvalidParameter)
        }
    }
}

impl TryFrom<bt_hci::param::BdAddr> for BluetoothAddress {
    type Error = BluetoothError;

    fn try_from(bd_addr: bt_hci::param::BdAddr) -> Result<Self, Self::Error> {
        bd_addr.raw().try_into()
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

impl Default for BluetoothHost {
    fn default() -> Self {
        Self {
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
        }
    }
}

impl BluetoothHost {
    /// Create a new BluetoothHost with default values
    pub fn new() -> Self {
        Self::default()
    }
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
    /// Get local Bluetooth information
    GetLocalInfo,
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
    /// Local Bluetooth information
    LocalInfo(LocalDeviceInfo),
    /// Error occurred
    Error(BluetoothError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bluetooth_address_creation() {
        let addr = BluetoothAddress::new([0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC]);
        assert_eq!(addr.as_bytes(), &[0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC]);
    }

    #[test]
    fn test_bluetooth_address_format_hex() {
        let addr = BluetoothAddress::new([0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC]);
        let formatted = addr.format_hex();
        assert_eq!(formatted.as_str(), "12:34:56:78:9A:BC");
    }

    #[test]
    fn test_bluetooth_address_format_hex_edge_cases() {
        // Test zero address
        let addr_zero = BluetoothAddress::new([0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
        assert_eq!(addr_zero.format_hex().as_str(), "00:00:00:00:00:00");

        // Test max address
        let addr_max = BluetoothAddress::new([0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
        assert_eq!(addr_max.format_hex().as_str(), "FF:FF:FF:FF:FF:FF");

        // Test mixed case
        let addr_mixed = BluetoothAddress::new([0x0A, 0xB1, 0x2C, 0xD3, 0x4E, 0xF5]);
        assert_eq!(addr_mixed.format_hex().as_str(), "0A:B1:2C:D3:4E:F5");
    }

    #[test]
    fn test_bluetooth_address_conversions() {
        let bytes = [0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC];

        // Test From<[u8; 6]>
        let addr: BluetoothAddress = bytes.into();
        assert_eq!(addr.as_bytes(), &bytes);

        // Test Into<[u8; 6]>
        let converted_bytes: [u8; 6] = addr.into();
        assert_eq!(converted_bytes, bytes);
    }

    #[test]
    fn test_bluetooth_address_try_from_slice() {
        // Test valid slice
        let bytes = &[0x12u8, 0x34u8, 0x56u8, 0x78u8, 0x9Au8, 0xBCu8][..];
        let addr = BluetoothAddress::try_from(bytes).unwrap();
        assert_eq!(addr.as_bytes(), &[0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC]);

        // Test invalid lengths
        let bytes_short = &[0x12u8, 0x34u8, 0x56u8][..];
        let bytes_long = &[
            0x12u8, 0x34u8, 0x56u8, 0x78u8, 0x9Au8, 0xBCu8, 0xDEu8, 0xF0u8,
        ][..];

        assert!(BluetoothAddress::try_from(bytes_short).is_err());
        assert!(BluetoothAddress::try_from(bytes_long).is_err());
    }

    #[test]
    fn test_bluetooth_device_creation() {
        let addr = BluetoothAddress::new([0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC]);
        let device = BluetoothDevice::new(addr);

        assert_eq!(device.addr, addr);
        assert_eq!(device.rssi, None);
        assert_eq!(device.name, None);
        assert_eq!(device.class_of_device, None);
    }

    #[test]
    fn test_bluetooth_device_builder_pattern() {
        let addr = BluetoothAddress::new([0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC]);
        let name = [
            b'T', b'e', b's', b't', 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0,
        ];

        let device = BluetoothDevice::new(addr)
            .with_rssi(-50)
            .with_name(name)
            .with_class_of_device(0x240404);

        assert_eq!(device.addr, addr);
        assert_eq!(device.rssi, Some(-50));
        assert_eq!(device.name, Some(name));
        assert_eq!(device.class_of_device, Some(0x240404));
    }

    #[test]
    fn test_bluetooth_device_rssi_values() {
        let addr = BluetoothAddress::new([0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC]);

        // Test various RSSI values
        let device_weak = BluetoothDevice::new(addr).with_rssi(-80);
        let device_medium = BluetoothDevice::new(addr).with_rssi(-50);
        let device_strong = BluetoothDevice::new(addr).with_rssi(-20);

        assert_eq!(device_weak.rssi, Some(-80));
        assert_eq!(device_medium.rssi, Some(-50));
        assert_eq!(device_strong.rssi, Some(-20));
    }

    #[test]
    fn test_bluetooth_device_class_of_device() {
        let addr = BluetoothAddress::new([0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC]);

        // Test various device classes based on Bluetooth standards
        let audio_device = BluetoothDevice::new(addr).with_class_of_device(0x240404); // Audio: Headphones
        let phone_device = BluetoothDevice::new(addr).with_class_of_device(0x200404); // Phone: Smartphone  
        let computer_device = BluetoothDevice::new(addr).with_class_of_device(0x100104); // Computer: Desktop

        assert_eq!(audio_device.class_of_device, Some(0x240404));
        assert_eq!(phone_device.class_of_device, Some(0x200404));
        assert_eq!(computer_device.class_of_device, Some(0x100104));
    }

    #[test]
    fn test_bluetooth_device_name_handling() {
        let addr = BluetoothAddress::new([0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC]);

        // Test with various name lengths
        let short_name = [
            b'H', b'i', 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0,
        ];
        let full_name = [b'T'; 32]; // Full 32-byte name
        let empty_name = [0u8; 32]; // Empty name

        let device_short = BluetoothDevice::new(addr).with_name(short_name);
        let device_full = BluetoothDevice::new(addr).with_name(full_name);
        let device_empty = BluetoothDevice::new(addr).with_name(empty_name);

        assert_eq!(device_short.name, Some(short_name));
        assert_eq!(device_full.name, Some(full_name));
        assert_eq!(device_empty.name, Some(empty_name));
    }

    #[test]
    fn test_local_device_info_default() {
        let info = LocalDeviceInfo::default();

        assert_eq!(info.bd_addr, None);
        assert_eq!(info.hci_version, None);
        assert_eq!(info.hci_revision, None);
        assert_eq!(info.lmp_version, None);
        assert_eq!(info.manufacturer_name, None);
        assert_eq!(info.lmp_subversion, None);
        assert_eq!(info.local_features, None);
        assert_eq!(info.acl_data_packet_length, None);
        assert_eq!(info.sco_data_packet_length, None);
        assert_eq!(info.total_num_acl_data_packets, None);
        assert_eq!(info.total_num_sco_data_packets, None);
    }

    #[test]
    fn test_local_device_info_populated() {
        let addr = BluetoothAddress::new([0x00, 0x11, 0x22, 0x33, 0x44, 0x55]);
        let features = [0xFF, 0xFE, 0xCD, 0xFE, 0xDB, 0xFF, 0x7B, 0x87];

        let info = LocalDeviceInfo {
            bd_addr: Some(addr),
            hci_version: Some(0x0C), // Bluetooth 5.2
            hci_revision: Some(0x1234),
            lmp_version: Some(0x0C),
            manufacturer_name: Some(0x000F), // Broadcom
            lmp_subversion: Some(0x5678),
            local_features: Some(features),
            acl_data_packet_length: Some(1021),
            sco_data_packet_length: Some(64),
            total_num_acl_data_packets: Some(8),
            total_num_sco_data_packets: Some(8),
        };

        assert_eq!(info.bd_addr, Some(addr));
        assert_eq!(info.hci_version, Some(0x0C));
        assert_eq!(info.manufacturer_name, Some(0x000F));
        assert_eq!(info.acl_data_packet_length, Some(1021));
        assert_eq!(info.local_features, Some(features));
    }

    #[test]
    fn test_bluetooth_state_exhaustive() {
        // Test that all BluetoothState variants exist and can be matched
        let states = [
            BluetoothState::PoweredOff,
            BluetoothState::PoweredOn,
            BluetoothState::Discovering,
            BluetoothState::Connecting,
            BluetoothState::Connected,
        ];

        for state in states {
            match state {
                BluetoothState::PoweredOff => assert!(true),
                BluetoothState::PoweredOn => assert!(true),
                BluetoothState::Discovering => assert!(true),
                BluetoothState::Connecting => assert!(true),
                BluetoothState::Connected => assert!(true),
            }
        }
    }

    #[test]
    fn test_bluetooth_error_exhaustive() {
        // Test that all BluetoothError variants exist and implement required traits
        let errors = [
            BluetoothError::HciError,
            BluetoothError::DeviceNotFound,
            BluetoothError::DeviceNotConnected,
            BluetoothError::ConnectionFailed,
            BluetoothError::Timeout,
            BluetoothError::AlreadyInProgress,
            BluetoothError::InvalidParameter,
            BluetoothError::NotSupported,
            BluetoothError::DiscoveryFailed,
            BluetoothError::InitializationFailed,
            BluetoothError::TransportError,
            BluetoothError::InvalidState,
        ];

        for error in errors {
            // Test that Debug trait is implemented
            let _ = &error as &dyn core::fmt::Debug;

            // Test that Copy and Clone are implemented
            let _cloned = error;
            let _copied = error;

            // Test that PartialEq is implemented
            assert_eq!(error, error);
        }
    }

    #[test]
    fn test_bluetooth_host_initialization() {
        // The BluetoothHost is initialized as a static, so we can't create new instances easily
        // But we can test that the static is properly initialized
        // This is more of a compilation test
        let _host_ref = &BLUETOOTH_HOST;
    }

    #[test]
    fn test_channels_initialization() {
        // Test that the static channels exist and have correct types
        let _request_sender = REQUEST_CHANNEL.sender();
        let _request_receiver = REQUEST_CHANNEL.receiver();
        let _response_sender = RESPONSE_CHANNEL.sender();
        let _response_receiver = RESPONSE_CHANNEL.receiver();
    }

    #[test]
    fn test_type_traits() {
        // Test that types implement expected traits
        let addr = BluetoothAddress::new([0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC]);
        let device = BluetoothDevice::new(addr);
        let info = LocalDeviceInfo::default();

        // Test Debug
        let _ = &addr as &dyn core::fmt::Debug;
        let _ = &device as &dyn core::fmt::Debug;
        let _ = &info as &dyn core::fmt::Debug;

        // Test Clone/Copy for address
        let _addr_clone = addr;
        let _addr_copy = addr;

        // Test Clone for device
        let _device_clone = device.clone();

        // Test PartialEq for address
        assert_eq!(addr, addr);

        // Test Hash for address (it's used in FnvIndexMap)
        use core::hash::{Hash, Hasher};

        // Create a simple hasher to test Hash implementation
        struct TestHasher(u64);
        impl Hasher for TestHasher {
            fn finish(&self) -> u64 {
                self.0
            }
            fn write(&mut self, bytes: &[u8]) {
                for &byte in bytes {
                    self.0 = self.0.wrapping_mul(31).wrapping_add(byte as u64);
                }
            }
        }

        let mut hasher = TestHasher(0);
        addr.hash(&mut hasher);
        let _hash = hasher.finish();
    }
}
