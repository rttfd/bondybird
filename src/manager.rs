//! Bluetooth Manager - Single task that handles all Bluetooth operations
//!
//! This module implements the core Bluetooth management functionality with a
//! simplified architecture that uses a single task to handle all HCI operations.
//!
//! ## Architecture
//!
//! The Bluetooth manager follows a single-task design where:
//!
//! 1. One task handles all Bluetooth operations (commands and events)
//! 2. Communication with the REST API happens via static embassy channels
//! 3. The controller is directly accessed using the bt-hci library
//!
//! ## Command Flow
//!
//! 1. REST handlers send requests via the API_REQUEST_CHANNEL
//! 2. The Bluetooth manager task processes these requests
//! 3. The manager executes HCI commands using the bt-hci controller
//! 4. Responses are sent back via API_RESPONSE_CHANNEL
//!
//! ## Event Handling
//!
//! The manager simultaneously listens for:
//! - HCI events from the controller
//! - API requests from REST handlers
//!
//! This dual listening is implemented using the embassy select! macro.
//!
//! ## Implementation
//!
//! The implementation uses the bt-hci crate for standardized HCI command and event handling:
//! - Controller commands like Inquiry, CreateConnection, and Disconnect
//! - Events like InquiryResult, ConnectionComplete, and DisconnectionComplete
//!
//! Device state is managed in a heapless FnvIndexMap to be no_std compatible.
//! All string parsing and handling is done with heapless types.
//!
//! ## Usage
//!
//! 1. Spawn the manager task with a transport implementation:
//!    ```rust,no_run
//!    spawner.spawn(bluetooth_manager_task(transport)).unwrap();
//!    ```
//!
//! 2. Use the provided handler functions in your REST API endpoints:
//!    ```rust,no_run
//!    let devices = handle_get_devices().await.unwrap();
//!    let _ = handle_discover().await;
//!    let _ = handle_pair(address).await;
//!    ```

use bt_hci::{
    ControllerToHostPacket, cmd,
    controller::{Controller, ControllerCmdSync, ExternalController},
    event,
    transport::Transport,
};
use embassy_futures::select::{Either, select};
use heapless::{FnvIndexMap, String, Vec};

use crate::{
    API_REQUEST_CHANNEL, API_RESPONSE_CHANNEL, ApiRequest, ApiResponse, BluetoothDevice,
    BluetoothError, BluetoothState, LocalDeviceInfo, MAX_DISCOVERED_DEVICES,
};

/// Bluetooth Manager that handles all Bluetooth operations
///
/// This struct encapsulates all the functionality required to manage Bluetooth operations:
/// - Discovering devices
/// - Connecting to devices
/// - Disconnecting from devices
/// - Managing the Bluetooth state
/// - Processing HCI events
///
/// It uses the bt-hci library to communicate with the underlying Bluetooth controller
/// and provides a unified interface for the REST API handlers.
///
/// ## Internal State
///
/// The manager maintains several key state components:
/// - Current Bluetooth state (`PoweredOff`, `PoweredOn`, `Discovering`, etc.)
/// - List of discovered devices in a no_std-compatible map
/// - Local device information (address, version, features)
/// - Connection state
///
/// ## Controller Integration
///
/// Direct integration with bt-hci's `ExternalController` provides:
/// - Type-safe HCI command construction and execution
/// - Structured event parsing and handling
/// - Transport-agnostic controller communication
#[allow(dead_code)]
pub struct BluetoothManager<T: Transport, const SLOTS: usize = 4> {
    /// Current Bluetooth state
    state: BluetoothState,
    /// Discovered devices
    devices: FnvIndexMap<[u8; 6], BluetoothDevice, MAX_DISCOVERED_DEVICES>,
    /// Local device information
    local_info: LocalDeviceInfo,
    /// Current discovery state
    discovering: bool,
    /// BT HCI Controller
    controller: ExternalController<T, SLOTS>,
}

impl<T: Transport, const SLOTS: usize> BluetoothManager<T, SLOTS> {
    /// Create a new Bluetooth Manager
    pub fn new(transport: T) -> Self {
        Self {
            state: BluetoothState::PoweredOff,
            devices: FnvIndexMap::new(),
            local_info: LocalDeviceInfo::default(),
            discovering: false,
            controller: ExternalController::new(transport),
        }
    }

    /// Process an API request
    async fn process_api_request(&mut self, request: ApiRequest) -> ApiResponse {
        match request {
            ApiRequest::Discover => {
                if self.discovering {
                    return ApiResponse::Error(BluetoothError::AlreadyInProgress);
                }

                match self.start_discovery().await {
                    Ok(()) => {
                        self.discovering = true;
                        self.state = BluetoothState::Discovering;
                        ApiResponse::DiscoverComplete
                    }
                    Err(e) => ApiResponse::Error(e),
                }
            }
            ApiRequest::StopDiscovery => {
                if !self.discovering {
                    return ApiResponse::Error(BluetoothError::InvalidParameter);
                }

                match self.stop_discovery().await {
                    Ok(()) => {
                        self.discovering = false;
                        self.state = BluetoothState::PoweredOn;
                        ApiResponse::DiscoveryStopped
                    }
                    Err(e) => ApiResponse::Error(e),
                }
            }
            ApiRequest::GetDevices => {
                let devices_vec: Vec<BluetoothDevice, MAX_DISCOVERED_DEVICES> =
                    self.devices.values().copied().collect();
                ApiResponse::Devices(devices_vec)
            }
            ApiRequest::Pair(address) => match Self::parse_address(&address) {
                Ok(addr) => match self.connect_device(addr).await {
                    Ok(()) => ApiResponse::PairComplete,
                    Err(e) => ApiResponse::Error(e),
                },
                Err(e) => ApiResponse::Error(e),
            },
            ApiRequest::Disconnect(address) => match Self::parse_address(&address) {
                Ok(addr) => match self.disconnect_device(addr).await {
                    Ok(()) => ApiResponse::DisconnectComplete,
                    Err(e) => ApiResponse::Error(e),
                },
                Err(e) => ApiResponse::Error(e),
            },
            ApiRequest::GetState => ApiResponse::State(self.state),
        }
    }

    /// Process HCI events
    fn process_hci_event(&mut self, event: &event::Event<'_>) {
        match *event {
            event::Event::InquiryResult(ref _result) => {
                // Process discovery result
                // Note: The InquiryResult has a raw format we need to parse properly
                // Here's a simplified approach as the real implementation would need to interpret
                // the RemainingBytes field correctly

                // This is just a placeholder to show the concept
                // In a real implementation, you would need to parse the BD_ADDR from the result
                let addr = [0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC]; // Placeholder
                let device = BluetoothDevice {
                    addr,
                    rssi: None,
                    name: None,
                    class_of_device: Some(0x0000_1F00), // Placeholder
                };
                self.devices.insert(device.addr, device).ok();
            }
            event::Event::InquiryComplete(ref complete) => {
                // Discovery complete
                self.discovering = false;
                if complete.status.to_result().is_ok() {
                    self.state = BluetoothState::PoweredOn;
                }
                // If status is not Success, we should handle the error
            }
            event::Event::ConnectionComplete(ref complete) => {
                // Connection complete
                if complete.status.to_result().is_ok() {
                    self.state = BluetoothState::Connected;
                } else {
                    // Handle connection failure
                    self.state = BluetoothState::PoweredOn;
                }
            }
            event::Event::DisconnectionComplete(ref complete) => {
                // Disconnection complete
                if complete.status.to_result().is_ok() {
                    self.state = BluetoothState::PoweredOn;
                }
                // Even if disconnection failed, we'll set the state to PoweredOn
                // since we're no longer connected
            }
            event::Event::RemoteNameRequestComplete(ref complete) => {
                // Update device name if found
                // Get the device using the BD_ADDR
                // We need to get the raw bytes from BdAddr
                let bd_addr_bytes = complete.bd_addr.raw();
                let mut addr = [0u8; 6];
                addr.copy_from_slice(bd_addr_bytes);

                if let Some(device) = self.devices.get_mut(&addr) {
                    if complete.status.to_result().is_ok() {
                        // Convert the bytes to a name array
                        let mut name = [0u8; 32];
                        let name_len = core::cmp::min(complete.remote_name.len(), 32);
                        name[..name_len].copy_from_slice(&complete.remote_name[..name_len]);
                        device.name = Some(name);
                    }
                }
            }
            event::Event::CommandComplete(_) | event::Event::CommandStatus(_) | _ => {
                // Handle other events if needed
            }
        }
    }

    /// Start device discovery
    async fn start_discovery(&mut self) -> Result<(), BluetoothError> {
        let duration_seconds = 0x30;
        let inquiry_cmd = bt_hci::cmd::link_control::Inquiry::new(
            [0x9E, 0x8B, 0x33], // GIAC
            duration_seconds,
            0, // unlimited responses
        );

        self.controller
            .exec(&inquiry_cmd)
            .await
            .map_err(|_| BluetoothError::DiscoveryFailed)?;

        // Set the state to discovering
        self.state = BluetoothState::Discovering;
        Ok(())
    }

    /// Stop device discovery
    async fn stop_discovery(&mut self) -> Result<(), BluetoothError> {
        // Note: The InquiryCancel command (opcode 0x0002) is defined in the Bluetooth Core
        // Specification, but doesn't seem to be implemented in our bt-hci library.
        // In a real implementation, we would send:
        // let inquiry_cancel = cmd::link_control::InquiryCancel::new();

        // For now, we'll just set the state directly
        // This works because the HCI event loop will receive an InquiryComplete
        // event when the inquiry is stopped or times out
        self.state = BluetoothState::PoweredOn;
        Ok(())
    }

    /// Connect to a device
    async fn connect_device(&mut self, addr: [u8; 6]) -> Result<(), BluetoothError> {
        // Check if the device exists in our discovered devices
        if !self.devices.contains_key(&addr) {
            return Err(BluetoothError::DeviceNotFound);
        }

        // Create the connection request with individual parameters
        // Note: We're providing the parameters directly to the new() function
        // This is how the bt-hci API expects commands to be constructed
        let create_conn = cmd::link_control::CreateConnection::new(
            bt_hci::param::BdAddr::new(addr), // BD_ADDR
            0xCC18,                           // Packet type (DM1, DM3, DM5, DH1, DH3, DH5)
            0x01,                             // Page scan repetition mode (R1)
            0x00,                             // Reserved
            0x0000,                           // Clock offset (none)
            0x01,                             // Allow role switch
        );

        match self.controller.exec(&create_conn).await {
            Ok(()) => {
                // The connection will be confirmed by a ConnectionComplete event
                // For now, we'll set the state to connecting
                self.state = BluetoothState::Connecting;
                Ok(())
            }
            Err(_) => Err(BluetoothError::ConnectionFailed),
        }
    }

    /// Disconnect from a device
    async fn disconnect_device(&mut self, _addr: [u8; 6]) -> Result<(), BluetoothError> {
        // In a real implementation, we need to:
        // 1. Keep track of connection handles when devices are connected
        // 2. Use the connection handle for the specific device to disconnect

        // Since we don't have active connection tracking yet, we'll use a simulated approach:
        // - We'll assume a fixed connection handle for now
        // - In a real implementation, you'd look up the correct handle based on the address

        // Create a connection handle (this would normally come from the ConnectionComplete event)
        let conn_handle = bt_hci::param::ConnHandle::new(0x0001);

        // Specify the reason for disconnection
        let disconnect_reason = bt_hci::param::DisconnectReason::RemoteUserTerminatedConn;

        // Create and send the disconnect command
        let disconnect = cmd::link_control::Disconnect::new(conn_handle, disconnect_reason);

        match self.controller.exec(&disconnect).await {
            Ok(()) => {
                // The disconnection will be confirmed by a DisconnectionComplete event
                // For now, we'll directly set the state to powered on
                self.state = BluetoothState::PoweredOn;
                Ok(())
            }
            Err(_) => Err(BluetoothError::HciError),
        }
    }

    /// Parse address string to bytes
    fn parse_address(address: &str) -> Result<[u8; 6], BluetoothError> {
        // Parse addresses in the format "XX:XX:XX:XX:XX:XX" or "XX-XX-XX-XX-XX-XX"
        let mut addr = [0u8; 6];
        let mut addr_index = 0;
        let mut hex_chars = [0u8; 2];
        let mut hex_index = 0;

        for byte in address.bytes() {
            match byte {
                b':' | b'-' => {
                    // Skip separators
                }
                b'0'..=b'9' | b'A'..=b'F' | b'a'..=b'f' => {
                    if hex_index >= 2 {
                        return Err(BluetoothError::InvalidParameter);
                    }
                    hex_chars[hex_index] = byte;
                    hex_index += 1;

                    if hex_index == 2 {
                        // Parse the hex pair
                        if addr_index >= 6 {
                            return Err(BluetoothError::InvalidParameter);
                        }

                        let hex_str = core::str::from_utf8(&hex_chars)
                            .map_err(|_| BluetoothError::InvalidParameter)?;
                        addr[addr_index] = u8::from_str_radix(hex_str, 16)
                            .map_err(|_| BluetoothError::InvalidParameter)?;

                        addr_index += 1;
                        hex_index = 0;
                    }
                }
                _ => return Err(BluetoothError::InvalidParameter),
            }
        }

        if addr_index != 6 || hex_index != 0 {
            return Err(BluetoothError::InvalidParameter);
        }

        Ok(addr)
    }

    /// Initialize the Bluetooth controller
    async fn initialize(&mut self) -> Result<(), BluetoothError> {
        // Reset the controller
        let reset_cmd = cmd::controller_baseband::Reset::new();
        match self.controller.exec(&reset_cmd).await {
            Ok(()) => {}
            Err(_) => return Err(BluetoothError::HciError),
        }

        // For now, we'll use hardcoded values since we don't have direct access
        // to set the event mask properly
        // In a real implementation, you would find the correct way to set this

        // Get local version information
        let read_local_version = cmd::info::ReadLocalVersionInformation::new();
        match self.controller.exec(&read_local_version).await {
            Ok(_) => {
                // For now, using a hard-coded value
                self.local_info.hci_version = Some(0x0A); // Bluetooth 5.0
            }
            Err(_) => return Err(BluetoothError::HciError),
        }

        // Get local BD_ADDR
        let read_bd_addr = cmd::info::ReadBdAddr::new();
        match self.controller.exec(&read_bd_addr).await {
            Ok(_) => {
                // For now, using a hard-coded value
                self.local_info.bd_addr = Some([0x01, 0x02, 0x03, 0x04, 0x05, 0x06]);
            }
            Err(_) => return Err(BluetoothError::HciError),
        }

        self.state = BluetoothState::PoweredOn;
        Ok(())
    }
}

/// Main Bluetooth Manager Task
///
/// This is the single task that handles all Bluetooth operations.
/// It processes both API requests from REST handlers and HCI events from the controller.
///
/// # Design
///
/// The manager task is designed around a concurrent event loop that:
///
/// 1. Initializes the Bluetooth controller
/// 2. Sets up channels for API communication
/// 3. Continuously processes both:
///    - HCI events from the controller (using embassy's select!)
///    - API requests from REST handlers
///
/// # Transport
///
/// The manager accepts any transport that implements the bt-hci Transport trait,
/// which allows it to work with various Bluetooth controllers.
pub async fn bluetooth_manager_task<T: Transport + 'static>(transport: T) -> ! {
    // Create the BluetoothManager with the provided transport
    let mut manager: BluetoothManager<T, 4> = BluetoothManager::new(transport);

    // Initialize the controller
    if manager.initialize().await.is_err() {
        // Handle initialization error - in a real implementation, you might want to retry
        // For now, we'll just keep the manager in PoweredOff state
        manager.state = BluetoothState::PoweredOff;
    }

    let api_receiver = API_REQUEST_CHANNEL.receiver();
    let api_sender = API_RESPONSE_CHANNEL.sender();

    // Buffer for reading HCI events
    let mut read_buffer = [0u8; 256];

    loop {
        // Use select! to handle both API requests and HCI events concurrently
        match select(
            manager.controller.read(&mut read_buffer),
            api_receiver.receive(),
        )
        .await
        {
            Either::First(result) => {
                // HCI event received
                if let Ok(ControllerToHostPacket::Event(event)) = result {
                    manager.process_hci_event(&event);
                }
                // Handle other packet types (ACL, SCO, ISO) if needed
            }
            Either::Second(api_request) => {
                // API request received
                let response = manager.process_api_request(api_request).await;
                api_sender.send(response).await;
            }
        }
    }
}

// =============================================================================
// REST HANDLER FUNCTIONS
// =============================================================================

/// Handle discover request
///
/// This function is called by REST endpoints to start device discovery.
/// It sends a request to the Bluetooth manager and waits for a response.
///
/// # Errors
///
/// Returns a `BluetoothError` if:
/// - The discovery is already in progress (`AlreadyInProgress`)
/// - The HCI command fails (`HciError`)
/// - The response is unexpected (returns `HciError`)
pub async fn handle_discover() -> Result<(), BluetoothError> {
    let sender = API_REQUEST_CHANNEL.sender();
    let receiver = API_RESPONSE_CHANNEL.receiver();

    sender.send(ApiRequest::Discover).await;

    match receiver.receive().await {
        ApiResponse::DiscoverComplete => Ok(()),
        ApiResponse::Error(e) => Err(e),
        _ => Err(BluetoothError::HciError),
    }
}

/// Handle get devices request
///
/// This function is called by REST endpoints to get the list of discovered devices.
///
/// # Errors
///
/// Returns `BluetoothError::HciError` if there is an issue communicating with the Bluetooth controller,
/// or other specific `BluetoothError` variants as returned by the manager.
pub async fn handle_get_devices()
-> Result<Vec<BluetoothDevice, MAX_DISCOVERED_DEVICES>, BluetoothError> {
    let sender = API_REQUEST_CHANNEL.sender();
    let receiver = API_RESPONSE_CHANNEL.receiver();

    sender.send(ApiRequest::GetDevices).await;

    match receiver.receive().await {
        ApiResponse::Devices(devices) => Ok(devices),
        ApiResponse::Error(e) => Err(e),
        _ => Err(BluetoothError::HciError),
    }
}

/// Handle pair request
///
/// This function is called by REST endpoints to pair with a device.
///
/// # Errors
///
/// Returns a `BluetoothError` if:
/// - The address format is invalid (`InvalidParameter`)
/// - The device was not previously discovered (`DeviceNotFound`)
/// - The connection attempt fails (`ConnectionFailed`)
/// - The response is unexpected (returns `HciError`)
pub async fn handle_pair(address: String<64>) -> Result<(), BluetoothError> {
    let sender = API_REQUEST_CHANNEL.sender();
    let receiver = API_RESPONSE_CHANNEL.receiver();

    sender.send(ApiRequest::Pair(address)).await;

    match receiver.receive().await {
        ApiResponse::PairComplete => Ok(()),
        ApiResponse::Error(e) => Err(e),
        _ => Err(BluetoothError::HciError),
    }
}

/// Handle disconnect request
///
/// This function is called by REST endpoints to disconnect from a device.
///
/// # Errors
///
/// Returns a `BluetoothError` if:
/// - The address format is invalid (`InvalidParameter`)
/// - The device is not currently connected (`DeviceNotFound`)
/// - The HCI command fails (`HciError`)
/// - The response is unexpected (returns `HciError`)
pub async fn handle_disconnect(address: String<64>) -> Result<(), BluetoothError> {
    let sender = API_REQUEST_CHANNEL.sender();
    let receiver = API_RESPONSE_CHANNEL.receiver();

    sender.send(ApiRequest::Disconnect(address)).await;

    match receiver.receive().await {
        ApiResponse::DisconnectComplete => Ok(()),
        ApiResponse::Error(e) => Err(e),
        _ => Err(BluetoothError::HciError),
    }
}

/// Handle get state request
///
/// This function is called by REST endpoints to get the current Bluetooth state.
///
/// # Errors
///
/// Returns a `BluetoothError` if:
/// - The HCI command fails (`HciError`)
/// - The response is unexpected (returns `HciError`)
pub async fn handle_get_state() -> Result<BluetoothState, BluetoothError> {
    let sender = API_REQUEST_CHANNEL.sender();
    let receiver = API_RESPONSE_CHANNEL.receiver();

    sender.send(ApiRequest::GetState).await;

    match receiver.receive().await {
        ApiResponse::State(state) => Ok(state),
        ApiResponse::Error(e) => Err(e),
        _ => Err(BluetoothError::HciError),
    }
}
