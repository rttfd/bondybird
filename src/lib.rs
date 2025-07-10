#![no_std]
#![doc = include_str!("../README.md")]
#![warn(missing_docs)]
#![allow(
    dead_code,
    clippy::unused_async,
    clippy::large_enum_variant,
    clippy::too_many_lines
)]

//! # BondyBird - BR/EDR Bluetooth Host for Embedded Systems
//!
//! `BondyBird` provides a complete BR/EDR (Classic) Bluetooth Host implementation
//! designed for embedded systems using Embassy and `no_std` environments.
//!
//! ## Key Features
//!
//! - **Runtime Configuration**: Configure inquiry parameters at startup
//! - **Parallel Processing**: Separate tasks for HCI events and API requests
//! - **Thread-Safe**: Embassy mutex-based shared state management
//! - **Memory Efficient**: `no_std` compatible with heapless collections
//!
//! ## Getting Started
//!
//! 1. Initialize the BluetoothHost with desired configuration
//! 2. Spawn the processor tasks
//! 3. Use the API functions for Bluetooth operations
//!
//! ```rust,no_run
//! use bondybird::{init_bluetooth_host, BluetoothHostOptions, hci_event_processor, api_request_processor};
//!
//! # async fn example() -> Result<(), &'static str> {
//! // 1. Initialize with configuration
//! let options = BluetoothHostOptions::default();
//! init_bluetooth_host(options).await?;
//!
//! // 2. Spawn tasks (in your embassy main)
//! // spawner.spawn(hci_event_processor(controller)).unwrap();
//! // spawner.spawn(api_request_processor(controller)).unwrap();
//!
//! // 3. Use API functions
//! // bondybird::api::start_discovery().await?;
//! # Ok(())
//! # }
//! ```

pub mod api;
pub mod class_of_device;
pub mod constants;
pub mod host;
pub mod processor;

use crate::constants::{
    DEFAULT_INQUIRY_DURATION, GIAC, MAX_CHANNELS, MAX_DISCOVERED_DEVICES, UNLIMITED_RESPONSES,
};
use bt_hci::event::{ExtendedInquiryResult, InquiryResultItem};
use embassy_sync::channel::Channel;
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    mutex::{MappedMutexGuard, Mutex, MutexGuard},
};
use heapless::{FnvIndexMap, String, Vec};

pub use api::{
    connect_device, disconnect_device, get_device_name, get_devices, get_paired_devices, get_state,
    start_discovery,
};
pub use class_of_device::{
    ClassOfDevice, DeviceDescription, MajorDeviceClass, MajorServiceClasses,
};
pub use processor::{api_request_processor, hci_event_processor};

pub(crate) static REQUEST_CHANNEL: Channel<CriticalSectionRawMutex, Request, MAX_CHANNELS> =
    Channel::new();

pub(crate) static RESPONSE_CHANNEL: Channel<CriticalSectionRawMutex, Response, MAX_CHANNELS> =
    Channel::new();

/// Global BluetoothHost, initialized by client at runtime
pub(crate) static BLUETOOTH_HOST: Mutex<CriticalSectionRawMutex, Option<BluetoothHost>> =
    Mutex::new(None);

/// Initialize the global BluetoothHost with the given options.
///
/// This function must be called before using any API functions or spawning the processor tasks.
/// It sets up the global BluetoothHost instance with the specified configuration options.
///
/// # Arguments
///
/// * `options` - Configuration options for the BluetoothHost, including inquiry parameters
///
/// # Returns
///
/// * `Ok(())` if initialization was successful
/// * `Err("BluetoothHost already initialized")` if already initialized
///
/// # Example
///
/// ```rust,no_run
/// use bondybird::{init_bluetooth_host, BluetoothHostOptions};
///
/// # async fn example() -> Result<(), &'static str> {
/// // Initialize with default options
/// init_bluetooth_host(BluetoothHostOptions::default()).await?;
/// # Ok(())
/// # }
/// ```
pub async fn init_bluetooth_host(options: BluetoothHostOptions) -> Result<(), &'static str> {
    let mut guard = BLUETOOTH_HOST.lock().await;
    if guard.is_some() {
        return Err("BluetoothHost already initialized");
    }
    *guard = Some(BluetoothHost::with_options(options));
    Ok(())
}

/// Get a locked reference to the global BluetoothHost.
///
/// This function provides access to the shared BluetoothHost instance for internal use.
/// It returns a mapped mutex guard that provides direct access to the BluetoothHost.
///
/// # Returns
///
/// * `Ok(MappedMutexGuard)` if the BluetoothHost is initialized
/// * `Err("BluetoothHost not initialized")` if not yet initialized
///
/// # Note
///
/// This function is primarily intended for internal use by the processor tasks.
/// API users should use the functions in the `api` module instead.
pub async fn bluetooth_host<'a>()
-> Result<MappedMutexGuard<'a, CriticalSectionRawMutex, BluetoothHost>, &'static str> {
    let guard = BLUETOOTH_HOST.lock().await;
    if guard.is_none() {
        return Err("BluetoothHost not initialized");
    }
    Ok(MutexGuard::map(guard, |opt| opt.as_mut().unwrap()))
}

/// A Bluetooth Device Address (`BD_ADDR`) wrapper for type safety
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
#[derive(Debug, Clone)]
pub struct BluetoothDevice {
    /// Bluetooth device address (`BD_ADDR`)
    pub addr: BluetoothAddress,
    /// Received Signal Strength Indicator (RSSI) in dBm, if available
    pub rssi: Option<i8>,
    /// Device name, if available (up to 32 bytes)
    pub name: Option<String<32>>,
    /// Class of Device (`CoD`) indicating device type and capabilities
    pub class_of_device: Option<ClassOfDevice>,
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
    pub fn with_name(mut self, name: String<32>) -> Self {
        self.name = Some(name);
        self
    }

    /// Update device with new name information from bytes
    #[must_use]
    pub fn with_name_bytes(mut self, name_bytes: [u8; 32]) -> Self {
        if let Some(name) = Self::bytes_to_name_string(&name_bytes) {
            self.name = Some(name);
        }
        self
    }

    /// Convert a 32-byte array to a heapless String, stopping at null terminator
    ///
    /// This utility function handles the conversion from raw byte arrays (as used in
    /// Bluetooth protocols) to properly formatted UTF-8 strings.
    ///
    /// # Arguments
    ///
    /// * `name_bytes` - A 32-byte array containing the device name, possibly null-terminated
    ///
    /// # Returns
    ///
    /// * `Some(String<32>)` - If the bytes contain valid UTF-8 text
    /// * `None` - If the bytes are not valid UTF-8 or the conversion fails
    pub fn bytes_to_name_string(name_bytes: &[u8; 32]) -> Option<String<32>> {
        // Find null terminator or use full length
        let name_len = name_bytes.iter().position(|&b| b == 0).unwrap_or(32);
        let name_str = core::str::from_utf8(&name_bytes[..name_len]).ok()?;
        String::try_from(name_str).ok()
    }

    /// Update device with class of device information
    #[must_use]
    pub fn with_class_of_device(mut self, class_of_device: ClassOfDevice) -> Self {
        self.class_of_device = Some(class_of_device);
        self
    }

    /// Parse the Class of Device (`CoD`) from a 3-byte array
    #[must_use]
    pub fn parse_class_of_device(class_of_device: &[u8; 3]) -> Option<ClassOfDevice> {
        if class_of_device.iter().all(|&x| x == 0) {
            None
        } else {
            // Bluetooth uses little-endian byte order
            let raw = u32::from_le_bytes([
                class_of_device[0],
                class_of_device[1],
                class_of_device[2],
                0,
            ]);
            Some(ClassOfDevice::from_raw(raw))
        }
    }
}

impl TryFrom<InquiryResultItem> for BluetoothDevice {
    type Error = BluetoothError;

    fn try_from(item: InquiryResultItem) -> Result<Self, Self::Error> {
        Ok(Self {
            addr: item.bd_addr.try_into()?,
            rssi: item.rssi,
            name: None,
            class_of_device: item
                .class_of_device
                .and_then(|x| BluetoothDevice::parse_class_of_device(&x)),
        })
    }
}

impl TryFrom<&ExtendedInquiryResult<'_>> for BluetoothDevice {
    type Error = BluetoothError;

    fn try_from(item: &ExtendedInquiryResult<'_>) -> Result<Self, Self::Error> {
        Ok(Self {
            addr: item.bd_addr.try_into()?,
            rssi: None,
            name: None,
            class_of_device: BluetoothDevice::parse_class_of_device(&item.class_of_device),
        })
    }
}

/// Local device information collected during initialization
#[derive(Debug, Clone, Copy, Default)]
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
    /// Bluetooth is idle
    Idle,
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

/// Options for configuring a `BluetoothHost` instance
///
/// This structure allows runtime configuration of Bluetooth inquiry parameters,
/// enabling customization of device discovery behavior.
///
/// # Examples
///
/// ```rust
/// use bondybird::{BluetoothHostOptions, constants};
///
/// // Use default options
/// let default_options = BluetoothHostOptions::default();
///
/// // Custom configuration for faster discovery
/// let fast_discovery = BluetoothHostOptions {
///     lap: constants::GIAC,           // General Inquiry Access Code
///     inquiry_length: 5,              // 5 * 1.28s = 6.4 seconds
///     num_responses: 10,              // Stop after 10 devices found
/// };
/// ```
#[derive(Debug, Clone, Copy)]
pub struct BluetoothHostOptions {
    /// Local Area Protocol (LAP) for device discovery
    ///
    /// Determines which devices will respond to inquiry requests:
    /// - `GIAC` ([0x33, 0x8B, 0x9E]): General Inquiry - discovers all discoverable devices
    /// - `LIAC` ([0x00, 0x8B, 0x9E]): Limited Inquiry - discovers devices in limited discoverable mode
    pub lap: [u8; 3],
    /// Inquiry duration in 1.28s units (1-48)
    ///
    /// The actual discovery time will be `inquiry_length * 1.28` seconds.
    /// For example, a value of 8 results in approximately 10.24 seconds of discovery.
    pub inquiry_length: u8,
    /// Maximum number of responses to return during discovery (0-255)
    ///
    /// Set to 0 for unlimited responses. The inquiry will stop early if this
    /// many devices are discovered before the time limit is reached.
    pub num_responses: u8,
}

impl Default for BluetoothHostOptions {
    fn default() -> Self {
        Self {
            lap: GIAC,
            inquiry_length: DEFAULT_INQUIRY_DURATION,
            num_responses: UNLIMITED_RESPONSES,
        }
    }
}

/// Shared Bluetooth data structure containing state, devices, and connections
#[derive(Debug)]
pub struct BluetoothHost {
    /// Current Bluetooth state
    state: BluetoothState,
    /// Discovered devices
    devices: FnvIndexMap<BluetoothAddress, BluetoothDevice, MAX_DISCOVERED_DEVICES>,
    /// Connection handles for connected devices (`BD_ADDR` -> `ConnHandle`)
    connections: FnvIndexMap<BluetoothAddress, u16, MAX_CHANNELS>,
    /// Local device information
    local_info: LocalDeviceInfo,
    /// Current discovery state
    discovering: bool,
    /// Host configuration options
    options: BluetoothHostOptions,
}

impl BluetoothHost {
    /// Create a new `BluetoothHost` with default options
    #[must_use]
    pub fn new() -> Self {
        Self::with_options(BluetoothHostOptions::default())
    }

    /// Create a new `BluetoothHost` with custom options
    #[must_use]
    pub fn with_options(options: BluetoothHostOptions) -> Self {
        Self {
            state: BluetoothState::PoweredOff,
            devices: FnvIndexMap::new(),
            connections: FnvIndexMap::new(),
            local_info: LocalDeviceInfo::default(),
            discovering: false,
            options,
        }
    }

    /// Get a reference to the options
    #[must_use]
    pub fn options(&self) -> &BluetoothHostOptions {
        &self.options
    }
}

impl Default for BluetoothHost {
    fn default() -> Self {
        Self::new()
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
    /// Get list of paired/connected devices
    GetPairedDevices,
    /// Get device name by address
    GetDeviceName(String<64>), // Device address as string
}

/// API responses sent back from the Bluetooth processing tasks
#[derive(Debug, Clone)]
pub enum Response {
    /// Discovery started successfully
    DiscoverStarted,
    /// Discovery completed successfully
    DiscoverComplete,
    /// Discovery stopped
    DiscoveryStopped,
    /// List of discovered devices
    Devices(Vec<BluetoothDevice, MAX_CHANNELS>),
    /// List of paired/connected devices
    PairedDevices(Vec<BluetoothDevice, MAX_CHANNELS>),
    /// Pairing completed successfully
    PairComplete,
    /// Disconnection completed
    DisconnectComplete,
    /// Current Bluetooth state
    State(BluetoothState),
    /// Local Bluetooth information
    LocalInfo(LocalDeviceInfo),
    /// Device name (up to 32 bytes)
    DeviceName(String<64>, String<32>), // Address and name
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
        let name_string = String::try_from("Test").unwrap();

        let device = BluetoothDevice::new(addr)
            .with_rssi(-50)
            .with_name(name_string.clone())
            .with_class_of_device(ClassOfDevice::from_raw(0x240404));

        assert_eq!(device.addr, addr);
        assert_eq!(device.rssi, Some(-50));
        assert_eq!(device.name, Some(name_string));
        assert_eq!(
            device.class_of_device,
            Some(ClassOfDevice::from_raw(0x240404))
        );
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
        let audio_device =
            BluetoothDevice::new(addr).with_class_of_device(ClassOfDevice::from_raw(0x240404)); // Audio: Headphones
        let phone_device =
            BluetoothDevice::new(addr).with_class_of_device(ClassOfDevice::from_raw(0x200404)); // Phone: Smartphone  
        let computer_device =
            BluetoothDevice::new(addr).with_class_of_device(ClassOfDevice::from_raw(0x100104)); // Computer: Desktop

        assert_eq!(
            audio_device.class_of_device,
            Some(ClassOfDevice::from_raw(0x240404))
        );
        assert_eq!(
            phone_device.class_of_device,
            Some(ClassOfDevice::from_raw(0x200404))
        );
        assert_eq!(
            computer_device.class_of_device,
            Some(ClassOfDevice::from_raw(0x100104))
        );
    }

    #[test]
    fn test_bluetooth_device_name_handling() {
        let addr = BluetoothAddress::new([0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC]);

        // Test with various name lengths using the new string API
        let short_name_bytes = [
            b'H', b'i', 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0,
        ];
        let full_name_str = "TTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTT"; // 32 'T' characters
        let empty_name_bytes = [0u8; 32]; // Empty name

        let device_short = BluetoothDevice::new(addr).with_name_bytes(short_name_bytes);
        let device_full =
            BluetoothDevice::new(addr).with_name(String::try_from(full_name_str).unwrap());
        let device_empty = BluetoothDevice::new(addr).with_name_bytes(empty_name_bytes);

        assert_eq!(device_short.name, Some(String::try_from("Hi").unwrap()));
        assert_eq!(
            device_full.name,
            Some(String::try_from(full_name_str).unwrap())
        );
        assert_eq!(device_empty.name, Some(String::try_from("").unwrap()));
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
        let _device_clone = device;

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

    #[test]
    fn test_bluetooth_host_options() {
        // Test default options
        let default_options = BluetoothHostOptions::default();
        assert_eq!(default_options.lap, GIAC);
        assert_eq!(default_options.inquiry_length, DEFAULT_INQUIRY_DURATION);
        assert_eq!(default_options.num_responses, UNLIMITED_RESPONSES);

        // Test custom options
        let custom_options = BluetoothHostOptions {
            lap: [0x00, 0x8B, 0x9E], // LIAC
            inquiry_length: 10,
            num_responses: 5,
        };
        assert_eq!(custom_options.lap, [0x00, 0x8B, 0x9E]);
        assert_eq!(custom_options.inquiry_length, 10);
        assert_eq!(custom_options.num_responses, 5);

        // Test that BluetoothHost can be created with options
        let host = BluetoothHost::with_options(custom_options);
        assert_eq!(host.options().lap, custom_options.lap);
        assert_eq!(host.options().inquiry_length, custom_options.inquiry_length);
        assert_eq!(host.options().num_responses, custom_options.num_responses);
    }

    #[test]
    fn test_class_of_device_parsing() {
        // Use 0x000404 which represents:
        // - Major Service Classes: 0x000 (no services)
        // - Major Device Class: 0x04 (Audio/Video)
        // - Minor Device Class: 0x01 (Wearable headset device)
        // - Format Type: 0x00
        let cod = ClassOfDevice::from_raw(0x000404);

        // Test major class
        assert_eq!(cod.major_device_class(), MajorDeviceClass::AudioVideo);

        // Test minor class
        assert_eq!(cod.minor_device_class(), 0x01);

        // Test service classes (should be empty for 0x000404)
        assert!(!cod.major_service_classes().audio());

        // Test human-readable format
        let desc = cod.description();
        assert_eq!(desc.major_class, "Audio/Video");
        assert_eq!(desc.minor_class, Some("Wearable headset device"));
    }
    #[test]
    fn test_class_of_device_display_trait() {
        let cod = ClassOfDevice::from_raw(0x000404);

        // Test Display implementation in a no_std compatible way
        let mut buffer = heapless::String::<64>::new();
        use core::fmt::Write;
        write!(buffer, "{cod}").unwrap();
        assert_eq!(buffer.as_str(), "Audio/Video (Wearable headset device)");
    }

    #[test]
    fn test_class_of_device_exhaustive() {
        // Test that all ClassOfDevice variants exist and implement required traits
        let cod_values = [
            0x000000, 0x000001, 0x000002, 0x000004, 0x000008, 0x000010, 0x000020, 0x000040,
            0x000080, 0x000100, 0x000200, 0x000400, 0x000800, 0x001000, 0x002000, 0x004000,
            0x008000, 0x010000, 0x020000, 0x040000, 0x080000, 0x100000, 0x200000, 0x400000,
            0x800000, 0xFFFFFF,
        ];

        for &value in &cod_values {
            let cod = ClassOfDevice::from_raw(value);

            // Test that Debug trait is implemented
            let _ = &cod as &dyn core::fmt::Debug;

            // Test that Copy and Clone are implemented
            let _cloned = cod;
            let _copied = cod;

            // Test that PartialEq is implemented
            assert_eq!(cod, cod);
        }
    }
}
