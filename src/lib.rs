#![no_std]
#![doc = include_str!("../README.md")]
#![warn(missing_docs)]
#![allow(dead_code, clippy::unused_async, clippy::too_many_lines)]

mod address;
pub mod api;
mod class_of_device;
pub mod constants;
mod host;
pub mod l2cap;
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

pub use address::BluetoothAddress;
pub use class_of_device::{
    ClassOfDevice, DeviceDescription, MajorDeviceClass, MajorServiceClasses,
};

pub(crate) static REQUEST_CHANNEL: Channel<CriticalSectionRawMutex, Request, MAX_CHANNELS> =
    Channel::new();

pub(crate) static RESPONSE_CHANNEL: Channel<CriticalSectionRawMutex, Response, MAX_CHANNELS> =
    Channel::new();

pub(crate) static INTERNAL_COMMAND_CHANNEL: Channel<
    CriticalSectionRawMutex,
    InternalCommand,
    MAX_CHANNELS,
> = Channel::new();

/// Global `BluetoothHost`, initialized by client at runtime
pub(crate) static BLUETOOTH_HOST: Mutex<CriticalSectionRawMutex, Option<BluetoothHost>> =
    Mutex::new(None);

/// Initialize the global `BluetoothHost` with the given options.
///
/// This function must be called before using any API functions or spawning the processor tasks.
/// It sets up the global `BluetoothHost` instance with the specified configuration options.
///
/// # Arguments
///
/// * `options` - Configuration options for the `BluetoothHost`, including inquiry parameters
///
/// # Returns
///
/// * `Ok(())` if initialization was successful
/// * `Err("BluetoothHost already initialized")` if already initialized
///
/// # Errors
///
/// This function will return an error if the `BluetoothHost` has already been initialized.
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

/// Get a locked reference to the global `BluetoothHost`.
///
/// This function provides access to the shared `BluetoothHost` instance for internal use.
/// It returns a mapped mutex guard that provides direct access to the `BluetoothHost`.
///
/// # Returns
///
/// * `Ok(MappedMutexGuard)` if the `BluetoothHost` is initialized
/// * `Err("BluetoothHost not initialized")` if not yet initialized
///
/// # Errors
///
/// This function will return an error if the `BluetoothHost` has not been initialized.
///
/// # Panics
///
/// This function panics if the mutex guard cannot be mapped (should never happen in practice).
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
    #[must_use]
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
    /// Bluetooth is idle
    Idle,
}

/// Bluetooth-related errors with detailed error information
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BluetoothError {
    /// HCI controller communication error with specific error code
    HciError(u8),
    /// HCI command failed with status
    HciCommandFailed(u8),
    /// HCI transport error
    HciTransportError,
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
    /// Stored link keys for paired devices (`BD_ADDR` -> Link Key)
    link_keys: FnvIndexMap<BluetoothAddress, [u8; 16], MAX_CHANNELS>,
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
            link_keys: FnvIndexMap::new(),
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

    /// Store a link key for a device
    ///
    /// # Errors
    /// This method returns an error if the link key cannot be stored
    /// (e.g., if the address already exists).
    ///
    pub fn store_link_key(
        &mut self,
        addr: BluetoothAddress,
        link_key: [u8; 16],
    ) -> Result<(), BluetoothError> {
        self.link_keys
            .insert(addr, link_key)
            .map(|_| ())
            .map_err(|_| BluetoothError::HciError(0xFF)) // Generic storage error
    }

    /// Get a link key for a device
    #[must_use]
    pub fn get_link_key(&self, addr: &BluetoothAddress) -> Option<&[u8; 16]> {
        self.link_keys.get(addr)
    }

    /// Remove a link key for a device
    pub fn remove_link_key(&mut self, addr: &BluetoothAddress) -> Option<[u8; 16]> {
        self.link_keys.remove(addr)
    }

    /// Check if a device has a stored link key
    #[must_use]
    pub fn has_link_key(&self, addr: &BluetoothAddress) -> bool {
        self.link_keys.contains_key(addr)
    }
}

impl Default for BluetoothHost {
    fn default() -> Self {
        Self::new()
    }
}

/// API requests sent to the Bluetooth processing tasks
#[derive(Debug, Clone)]
pub(crate) enum Request {
    /// Start device discovery
    Discover,
    /// Stop ongoing discovery
    StopDiscovery,
    /// Get list of discovered devices
    GetDevices,
    /// Connect/pair with a device
    Pair(BluetoothAddress),
    /// Disconnect from a device
    Disconnect(BluetoothAddress),
    /// Get current Bluetooth state
    GetState,
    /// Get local Bluetooth information
    GetLocalInfo,
    /// Get list of paired/connected devices
    GetPairedDevices,
    /// Get device name by address
    GetDeviceName(BluetoothAddress),
}

/// Internal async commands from event processor (fire-and-forget, no response needed)
#[derive(Debug, Clone)]
pub(crate) enum InternalCommand {
    /// Send Authentication Requested command
    AuthenticationRequested {
        conn_handle: u16,
    },
    /// Send Link Key Request Reply
    LinkKeyRequestReply {
        bd_addr: BluetoothAddress,
        link_key: [u8; 16],
    },
    /// Send Link Key Request Negative Reply
    LinkKeyRequestNegativeReply {
        bd_addr: BluetoothAddress,
    },
    /// Send IO Capability Request Reply
    IoCapabilityRequestReply {
        bd_addr: BluetoothAddress,
    },
    /// Send User Confirmation Request Reply
    UserConfirmationRequestReply {
        bd_addr: BluetoothAddress,
    },
    /// Send PIN Code Request Reply
    PinCodeRequestReply {
        bd_addr: BluetoothAddress,
        pin_code: [u8; 16],
    },
    /// Host state mutations (added for event.rs decoupling)
    UpsertDevice(BluetoothDevice),
    SetDiscovering(bool),
    SetState(BluetoothState),
    AddConnection(BluetoothAddress, u16),
    RemoveConnection(u16),
    UpdateDeviceName(
        BluetoothAddress,
        [u8; crate::constants::MAX_DEVICE_NAME_LENGTH],
    ),
    StoreLinkKey(BluetoothAddress, [u8; 16]),
}

/// API responses sent back from the Bluetooth processing tasks
#[derive(Debug, Clone)]
pub(crate) enum Response {
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
    DeviceName(BluetoothAddress, String<32>), // Address and name
    /// Error occurred
    Error(BluetoothError),
}

#[cfg(test)]
mod tests {
    use super::*;

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
            .with_class_of_device(ClassOfDevice::from_raw(0x0024_0404));

        assert_eq!(device.addr, addr);
        assert_eq!(device.rssi, Some(-50));
        assert_eq!(device.name, Some(name_string));
        assert_eq!(
            device.class_of_device,
            Some(ClassOfDevice::from_raw(0x0024_0404))
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
            BluetoothDevice::new(addr).with_class_of_device(ClassOfDevice::from_raw(0x0024_0404)); // Audio: Headphones
        let phone_device =
            BluetoothDevice::new(addr).with_class_of_device(ClassOfDevice::from_raw(0x0020_0404)); // Phone: Smartphone  
        let computer_device =
            BluetoothDevice::new(addr).with_class_of_device(ClassOfDevice::from_raw(0x0010_0104)); // Computer: Desktop

        assert_eq!(
            audio_device.class_of_device,
            Some(ClassOfDevice::from_raw(0x0024_0404))
        );
        assert_eq!(
            phone_device.class_of_device,
            Some(ClassOfDevice::from_raw(0x0020_0404))
        );
        assert_eq!(
            computer_device.class_of_device,
            Some(ClassOfDevice::from_raw(0x0010_0104))
        );
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
}
