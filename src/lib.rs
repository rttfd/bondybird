#![no_std]
#![doc = "BondyBird - A Rust library for BR/EDR (Classic) Bluetooth in embedded systems"]
#![warn(missing_docs)]
#![allow(dead_code)]

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
//! for standardized HCI command and event handling.
//!

mod event_handler;
mod manager;
mod stack;

pub use event_handler::BluetoothEventHandler;
pub use manager::BluetoothManager;
pub use stack::{BluetoothControl, BluetoothStack};

// Channel-Based Bluetooth Manager for Embassy
//
// Architecture:
// - BluetoothManager: Receives HCI events → Sends high-level events to channel
// - EventHandler: Receives events from channel → Sends HCI commands via controller
// - Clean separation of concerns with async message passing

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, Receiver, Sender};
use heapless::{String, Vec};

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
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BluetoothState {
    /// Bluetooth is powered off or not initialized
    PoweredOff,
    /// Bluetooth is initializing
    Initializing,
    /// Bluetooth is ready and idle
    Idle,
    /// Currently discovering devices
    Discovering,
    /// Currently establishing a connection
    Connecting,
    /// Connected to one or more devices
    Connected,
    /// Error state
    Error,
}

/// Error types that can occur during Bluetooth operations
#[derive(Debug, Clone, Copy)]
pub enum BluetoothError {
    /// Bluetooth system is not initialized
    NotInitialized,
    /// Operation attempted in invalid state
    InvalidState,
    /// Requested device was not found
    DeviceNotFound,
    /// Connection attempt failed
    ConnectionFailed,
    /// Device discovery failed
    DiscoveryFailed,
    /// Internal buffer is full
    BufferFull,
    /// HCI controller error
    HciError,
    /// Communication channel is full
    ChannelFull,
}

// =============================================================================
// EVENTS AND COMMANDS
// =============================================================================

/// High-level events sent from Manager to `EventHandler`
#[derive(Debug, Clone)]
pub enum BluetoothEvent {
    // State events
    /// Bluetooth state has changed
    StateChanged(BluetoothState),

    // Initialization events
    /// System reset completed successfully
    ResetComplete,
    /// Local device information has been read
    LocalInfoComplete(LocalDeviceInfo),
    /// Event mask configuration completed
    EventMaskSet,

    // Discovery events
    /// A new device was discovered during inquiry
    DeviceDiscovered(BluetoothDevice),
    /// Device discovery process completed
    DiscoveryComplete,

    // Connection events
    /// Successfully connected to a device
    DeviceConnected {
        /// Address of the connected device
        addr: [u8; 6],
        /// Connection handle assigned by the controller
        handle: u16,
    },
    /// Device disconnected
    DeviceDisconnected {
        /// Address of the disconnected device
        addr: [u8; 6],
        /// Connection handle that was released
        handle: u16,
    },
    /// Connection attempt failed
    ConnectionFailed {
        /// Address of the device that failed to connect
        addr: [u8; 6],
        /// Reason code for the failure
        reason: u8,
    },

    // Data events
    /// Data received from a connected device
    DataReceived {
        /// Address of the device that sent the data
        from: [u8; 6],
        /// Connection handle the data was received on
        handle: u16,
        /// The received data payload
        data: Vec<u8, 64>,
    },

    // Error events
    /// An error occurred
    Error(BluetoothError),

    // Internal events for coordination
    /// Manager is ready and initialized
    ManagerReady,
    /// Event handler is ready
    HandlerReady,
}

/// Commands sent from `EventHandler` back to Manager
#[derive(Debug, Clone)]
pub enum BluetoothCommand {
    // System commands
    /// Initialize the Bluetooth system
    Initialize,
    /// Reset the Bluetooth controller
    Reset,
    /// Read local version information
    ReadLocalVersionInfo,
    /// Read local supported features
    ReadLocalSupportedFeatures,
    /// Read buffer size information
    ReadBufferSize,
    /// Read local Bluetooth device address
    ReadBdAddr,

    // Discovery commands
    /// Start device discovery for the specified duration
    StartDiscovery {
        /// Duration of discovery in seconds
        duration_seconds: u8,
    },
    /// Stop ongoing device discovery
    StopDiscovery,

    // Connection commands
    /// Connect to a specific device
    Connect {
        /// Address of the device to connect to
        addr: [u8; 6],
        /// Packet types allowed for this connection
        packet_types: u16,
        /// Page scan mode to use
        page_scan_mode: u8,
        /// Clock offset for faster connection
        clock_offset: u16,
        /// Whether to allow role switching
        allow_role_switch: bool,
    },
    /// Disconnect from a device
    Disconnect {
        /// Connection handle to disconnect
        handle: u16,
    },

    // Data commands
    /// Send data to a connected device
    SendData {
        /// Connection handle to send data on
        handle: u16,
        /// Data to send
        data: Vec<u8, 64>,
    },

    // Configuration commands
    /// Set the HCI event mask
    SetEventMask(u64),
    /// Configure scan mode
    SetScanMode {
        /// Whether the device should be discoverable
        discoverable: bool,
        /// Whether the device should be connectable
        connectable: bool,
    },
}

/// API commands for controlling the Bluetooth stack via external interfaces (e.g., REST API).
#[derive(Debug, Clone)]
pub enum ApiCommand {
    /// Start device discovery for the specified duration (in seconds).
    StartDiscovery {
        /// Duration of discovery in seconds.
        duration: u8,
    },
    /// Stop ongoing device discovery.
    StopDiscovery,
    /// Get the list of discovered devices.
    GetDiscoveredDevices,

    /// Connect to a device by address.
    ConnectDevice {
        /// Address of the device to connect to.
        addr: [u8; 6],
    },
    /// Disconnect from a device by address.
    DisconnectDevice {
        /// Address of the device to disconnect from.
        addr: [u8; 6],
    },
    /// Get the list of currently connected devices.
    GetConnectedDevices,

    /// Enable or disable auto-connect to a specific device.
    SetAutoConnect {
        /// Address of the target device.
        addr: [u8; 6],
        /// Whether auto-connect is enabled.
        enabled: bool,
    },

    /// Get the current Bluetooth stack status.
    GetStatus,
    /// Reset the Bluetooth stack.
    Reset,

    /// Send data to a connected device.
    SendData {
        /// Address of the target device.
        addr: [u8; 6],
        /// Data to send.
        data: Vec<u8, 64>,
    },
}

/// Responses sent from `EventHandler` back to REST API
/// API responses sent from the Bluetooth stack to external interfaces (e.g., REST API).
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum ApiResponse {
    /// The command completed successfully.
    Success,
    /// The command failed with an error message.
    Error {
        /// Error message.
        message: String<64>,
    },
    /// Status information about the Bluetooth stack.
    Status {
        /// Current Bluetooth state.
        state: BluetoothState,
        /// Number of discovered devices.
        discovered_count: usize,
        /// Number of connected devices.
        connected_count: usize,
    },
    /// List of discovered devices.
    DiscoveredDevices {
        /// Discovered Bluetooth devices.
        devices: Vec<BluetoothDevice, MAX_DISCOVERED_DEVICES>,
    },
    /// List of connected devices.
    ConnectedDevices {
        /// Connected devices as (address, handle) pairs.
        devices: Vec<([u8; 6], u16), MAX_CONNECTIONS>,
    },
    /// Data was sent successfully.
    DataSent,
}

// =============================================================================
// CHANNEL TYPES
// =============================================================================

// Internal processing channels, used for manager and handler communication
/// Channel for passing Bluetooth events from manager to handler
pub type EventChannel<const MAX: usize = 16> =
    Channel<CriticalSectionRawMutex, BluetoothEvent, MAX>;
/// Channel for passing commands from handler to manager
pub type CommandChannel<const MAX: usize = 16> =
    Channel<CriticalSectionRawMutex, BluetoothCommand, MAX>;

/// Sender end of the event channel
pub type EventSender<const MAX: usize = 16> =
    Sender<'static, CriticalSectionRawMutex, BluetoothEvent, MAX>;
/// Receiver end of the event channel
pub type EventReceiver<const MAX: usize = 16> =
    Receiver<'static, CriticalSectionRawMutex, BluetoothEvent, MAX>;

/// Sender end of the command channel
pub type CommandSender<const MAX: usize = 16> =
    Sender<'static, CriticalSectionRawMutex, BluetoothCommand, MAX>;
/// Receiver end of the command channel
pub type CommandReceiver<const MAX: usize = 16> =
    Receiver<'static, CriticalSectionRawMutex, BluetoothCommand, MAX>;

/// Exposed API for controlling the Bluetooth system.
/// Channel for passing API commands and responses
pub type ApiCommandChannel<const MAX: usize = 8> =
    Channel<CriticalSectionRawMutex, (ApiCommand, u32), MAX>; // (command, request_id)
/// Channel for passing API responses back
pub type ApiResponseChannel<const MAX: usize = 8> =
    Channel<CriticalSectionRawMutex, (ApiResponse, u32), MAX>; // (response, request_id)

/// Sender end of the API command channel
pub type ApiCommandSender<const MAX: usize = 8> =
    Sender<'static, CriticalSectionRawMutex, (ApiCommand, u32), MAX>;
/// Receiver end of the API command channel
pub type ApiCommandReceiver<const MAX: usize = 8> =
    Receiver<'static, CriticalSectionRawMutex, (ApiCommand, u32), MAX>;

/// Sender end of the API response channel
pub type ApiResponseSender<const MAX: usize = 8> =
    Sender<'static, CriticalSectionRawMutex, (ApiResponse, u32), MAX>;
/// Receiver end of the API response channel
pub type ApiResponseReceiver<const MAX: usize = 8> =
    Receiver<'static, CriticalSectionRawMutex, (ApiResponse, u32), MAX>;

// =============================================================================
// USAGE EXAMPLE
// =============================================================================

/// Example of how to use the channel-based Bluetooth system
pub async fn example_usage<T>(controller: T)
where
    T: bt_hci::controller::Controller
        + bt_hci::controller::ControllerCmdSync<bt_hci::cmd::controller_baseband::Reset>
        + bt_hci::controller::ControllerCmdSync<bt_hci::cmd::controller_baseband::SetEventMask>
        + bt_hci::controller::ControllerCmdSync<bt_hci::cmd::info::ReadLocalVersionInformation>
        + bt_hci::controller::ControllerCmdSync<bt_hci::cmd::info::ReadLocalSupportedFeatures>
        + bt_hci::controller::ControllerCmdSync<bt_hci::cmd::info::ReadBufferSize>
        + bt_hci::controller::ControllerCmdSync<bt_hci::cmd::info::ReadBdAddr>
        + bt_hci::controller::ControllerCmdSync<bt_hci::cmd::link_control::Inquiry>
        + bt_hci::controller::ControllerCmdSync<bt_hci::cmd::link_control::CreateConnection>
        + bt_hci::controller::ControllerCmdSync<bt_hci::cmd::link_control::Disconnect>
        + 'static,
{
    // Create channels
    static EVENT_CHANNEL: EventChannel = Channel::new();
    static COMMAND_CHANNEL: CommandChannel = Channel::new();
    static API_COMMAND_CHANNEL: ApiCommandChannel = Channel::new();
    static API_RESPONSE_CHANNEL: ApiResponseChannel = Channel::new();

    let event_sender = EVENT_CHANNEL.sender();
    let event_receiver = EVENT_CHANNEL.receiver();
    let command_sender = COMMAND_CHANNEL.sender();
    let command_receiver = COMMAND_CHANNEL.receiver();

    let api_command_receiver = API_COMMAND_CHANNEL.receiver();
    let api_response_sender = API_RESPONSE_CHANNEL.sender();

    // Uncomment if you want to use API command sender/receiver. e.g: REST API integration
    // let api_command_sender = API_COMMAND_CHANNEL.sender();
    // let api_response_receiver = API_RESPONSE_CHANNEL.receiver();

    // Create manager and handler
    let mut manager = BluetoothManager::new(controller, event_sender, command_receiver);
    let mut handler = BluetoothEventHandler::new(
        event_receiver,
        command_sender,
        api_command_receiver,
        api_response_sender,
    );

    // Configure auto-connect
    handler.set_auto_connect([0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC], true);

    // Spawn both tasks
    embassy_futures::join::join(manager.run(), handler.run()).await;
}
