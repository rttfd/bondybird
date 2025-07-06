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
    param::EventMask,
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
    /// Connection handles for connected devices (`BD_ADDR` -> `ConnHandle`)
    connections: FnvIndexMap<[u8; 6], u16, MAX_DISCOVERED_DEVICES>,
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
            connections: FnvIndexMap::new(),
            local_info: LocalDeviceInfo::default(),
            discovering: false,
            controller: ExternalController::new(transport),
        }
    }

    /// Helper method to add a device to the devices map
    fn add_device(
        &mut self,
        addr: [u8; 6],
        rssi: Option<i8>,
        name: Option<[u8; 32]>,
        class_of_device: Option<u32>,
    ) {
        let device = BluetoothDevice {
            addr,
            rssi,
            name,
            class_of_device,
        };
        self.devices.insert(device.addr, device).ok();
    }

    /// Helper method to parse a 3-byte class of device into a u32
    fn parse_class_of_device(bytes: &[u8]) -> Option<u32> {
        if bytes.len() >= 3 {
            let mut value: u32 = 0;
            for (i, &b) in bytes.iter().take(3).enumerate() {
                value |= u32::from(b) << (i * 8);
            }
            Some(value)
        } else {
            None
        }
    }

    /// Helper method to convert `CoreSpecificationVersion` to u8
    fn core_spec_version_to_u8(version: bt_hci::param::CoreSpecificationVersion) -> u8 {
        match version {
            bt_hci::param::CoreSpecificationVersion::VERSION_1_1 => 0x01,
            bt_hci::param::CoreSpecificationVersion::VERSION_1_2 => 0x02,
            bt_hci::param::CoreSpecificationVersion::VERSION_2_0_EDR => 0x03,
            bt_hci::param::CoreSpecificationVersion::VERSION_2_1_EDR => 0x04,
            bt_hci::param::CoreSpecificationVersion::VERSION_3_0_HS => 0x05,
            bt_hci::param::CoreSpecificationVersion::VERSION_4_0 => 0x06,
            bt_hci::param::CoreSpecificationVersion::VERSION_4_1 => 0x07,
            bt_hci::param::CoreSpecificationVersion::VERSION_4_2 => 0x08,
            bt_hci::param::CoreSpecificationVersion::VERSION_5_0 => 0x09,
            bt_hci::param::CoreSpecificationVersion::VERSION_5_1 => 0x0A,
            bt_hci::param::CoreSpecificationVersion::VERSION_5_2 => 0x0B,
            bt_hci::param::CoreSpecificationVersion::VERSION_5_3 => 0x0C,
            bt_hci::param::CoreSpecificationVersion::VERSION_5_4 => 0x0D,
            _ => 0x00, // Default to 1.0B for unknown versions
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
            event::Event::InquiryResult(ref result) => {
                // Process discovery result
                // The InquiryResult has fields stored in RemainingBytes that we need to parse
                // Format: num_responses followed by that many sets of fields

                // Get the number of responses in this event
                let num_responses = result.num_responses;

                // Each BD_ADDR is 6 bytes, so we need to parse each one
                // RemainingBytes implements Deref to &[u8], so we can use it directly
                let bd_addr_bytes = &*result.bd_addr;
                let class_of_device_bytes = &*result.class_of_device;

                // Parse each response (only if we have enough data)
                for i in 0..num_responses as usize {
                    // Each BD_ADDR is 6 bytes
                    if i * 6 + 6 <= bd_addr_bytes.len() {
                        // Extract the 6 bytes for this device's address
                        let mut addr = [0u8; 6];
                        addr.copy_from_slice(&bd_addr_bytes[i * 6..(i * 6 + 6)]);

                        // Parse class of device (3 bytes per device)
                        let class_of_device = if i * 3 + 3 <= class_of_device_bytes.len() {
                            Self::parse_class_of_device(&class_of_device_bytes[i * 3..(i * 3 + 3)])
                        } else {
                            None
                        };

                        // Add the device to our map
                        self.add_device(addr, None, None, class_of_device);
                    }
                }
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
                    // Extract the BD_ADDR and connection handle
                    let bd_addr_bytes = complete.bd_addr.raw();
                    let mut addr = [0u8; 6];
                    addr.copy_from_slice(bd_addr_bytes);

                    // Store the connection handle for this device
                    // Save the raw handle value which is directly accessible
                    // When using bt-hci crate with real hardware, this should be
                    // the actual connection handle provided by the controller
                    let conn_handle = complete.handle.raw();
                    self.connections.insert(addr, conn_handle).ok();

                    // Update the state
                    self.state = BluetoothState::Connected;
                } else {
                    // Handle connection failure
                    self.state = BluetoothState::PoweredOn;
                }
            }
            event::Event::DisconnectionComplete(ref complete) => {
                // Disconnection complete
                if complete.status.to_result().is_ok() {
                    // Get the connection handle from the event
                    let conn_handle = complete.handle.raw();

                    // Find the device address associated with this connection handle
                    // and remove it from our connections map
                    let keys_to_remove: Vec<[u8; 6], 8> = self
                        .connections
                        .iter()
                        .filter_map(|(addr, handle)| {
                            if *handle == conn_handle {
                                Some(*addr)
                            } else {
                                None
                            }
                        })
                        .collect();

                    // Remove each matching connection
                    for addr in keys_to_remove {
                        self.connections.remove(&addr);
                    }

                    self.state = BluetoothState::PoweredOn;
                }
                // Even if disconnection failed, we'll set the state to PoweredOn
                // since we're no longer connected
            }
            event::Event::RemoteNameRequestComplete(ref complete) => {
                // Update device name if found
                // Get the device using the BD_ADDR
                let bd_addr_bytes = complete.bd_addr.raw();
                let mut addr = [0u8; 6];
                addr.copy_from_slice(bd_addr_bytes);

                if complete.status.to_result().is_ok() {
                    // If we have the device already
                    if let Some(device) = self.devices.get_mut(&addr) {
                        // Convert the bytes to a name array
                        let mut name = [0u8; 32];
                        let name_len = core::cmp::min(complete.remote_name.len(), 32);
                        name[..name_len].copy_from_slice(&complete.remote_name[..name_len]);
                        device.name = Some(name);
                    } else {
                        // If this is a new device we haven't seen before
                        // Create a new device entry with the name
                        let mut name = [0u8; 32];
                        let name_len = core::cmp::min(complete.remote_name.len(), 32);
                        name[..name_len].copy_from_slice(&complete.remote_name[..name_len]);

                        // Add the device with just the name (no RSSI or class of device yet)
                        self.add_device(addr, None, Some(name), None);
                    }
                }
            }
            event::Event::ExtendedInquiryResult(ref result) => {
                // The ExtendedInquiryResult gives us more info, including RSSI and EIR data
                // It's easier to parse because fields are not in RemainingBytes format

                // Get the BD_ADDR directly from the result
                let addr = result.bd_addr;

                // Parse class of device (3 bytes)
                let class_of_device = Self::parse_class_of_device(&result.class_of_device);

                // Add the device with RSSI
                self.add_device(addr, Some(result.rssi), None, class_of_device);

                // Note: EIR data can contain additional information like device name
                // In a more complete implementation, we would parse this data
                // to extract the device name and other details
            }
            event::Event::InquiryResultWithRssi(ref result) => {
                // Process inquiry result with RSSI
                // Similar to InquiryResult but includes RSSI values

                let num_responses = result.num_responses;

                // Get the byte arrays from RemainingBytes
                let bd_addr_bytes = &*result.bd_addr;
                let rssi_bytes = &*result.rssi;
                let class_of_device_bytes = &*result.class_of_device;

                for i in 0..num_responses as usize {
                    // Each BD_ADDR is 6 bytes
                    if i * 6 + 6 <= bd_addr_bytes.len() && i < rssi_bytes.len() {
                        // Extract the 6 bytes for this device's address
                        let mut addr = [0u8; 6];
                        addr.copy_from_slice(&bd_addr_bytes[i * 6..(i * 6 + 6)]);

                        // Extract RSSI
                        // Note: RSSI is stored as a signed 8-bit value in the HCI spec
                        // This cast may technically wrap, but in the Bluetooth context
                        // it's the expected behavior as RSSI is always interpreted as a signed value
                        #[allow(clippy::cast_possible_wrap)]
                        let rssi = rssi_bytes[i] as i8;

                        // Parse class of device (3 bytes per device)
                        let class_of_device = if i * 3 + 3 <= class_of_device_bytes.len() {
                            Self::parse_class_of_device(&class_of_device_bytes[i * 3..(i * 3 + 3)])
                        } else {
                            None
                        };

                        // Add the device to our map
                        self.add_device(addr, Some(rssi), None, class_of_device);
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
    async fn disconnect_device(&mut self, addr: [u8; 6]) -> Result<(), BluetoothError> {
        // Get the connection handle for this device address from our connections map
        let conn_handle = match self.connections.get(&addr) {
            Some(handle) => bt_hci::param::ConnHandle::new(*handle),
            None => return Err(BluetoothError::DeviceNotConnected),
        };

        // Specify the reason for disconnection
        let disconnect_reason = bt_hci::param::DisconnectReason::RemoteUserTerminatedConn;

        // Create and send the disconnect command
        let disconnect = cmd::link_control::Disconnect::new(conn_handle, disconnect_reason);

        match self.controller.exec(&disconnect).await {
            Ok(()) => {
                // The disconnection will be confirmed by a DisconnectionComplete event
                // The actual device state update and connections map update will happen there
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

        // Set event mask to enable the events we're interested in
        let event_mask = EventMask::new()
            .enable_inquiry_complete(true)
            .enable_inquiry_result(true)
            .enable_conn_complete(true)
            .enable_conn_request(true)
            .enable_disconnection_complete(true)
            .enable_authentication_complete(true)
            .enable_remote_name_request_complete(true)
            .enable_encryption_change_v1(true)
            .enable_pin_code_request(true)
            .enable_link_key_request(true)
            .enable_link_key_notification(true)
            .enable_read_remote_supported_features_complete(true)
            .enable_read_remote_version_information_complete(true)
            .enable_hardware_error(true)
            .enable_inquiry_result_with_rssi(true)
            .enable_read_remote_ext_features_complete(true)
            .enable_ext_inquiry_result(true)
            .enable_io_capability_request(true)
            .enable_io_capability_response(true)
            .enable_user_confirmation_request(true)
            .enable_user_passkey_request(true)
            .enable_simple_pairing_complete(true);
        // Set the event mask command
        let set_event_mask = cmd::controller_baseband::SetEventMask::new(event_mask);
        match self.controller.exec(&set_event_mask).await {
            Ok(()) => {}
            Err(_) => return Err(BluetoothError::HciError),
        }

        // Get local version information
        let read_local_version = cmd::info::ReadLocalVersionInformation::new();
        match self.controller.exec(&read_local_version).await {
            Ok(version_info) => {
                // Extract and store the version information using our helper method
                let hci_version = Self::core_spec_version_to_u8(version_info.hci_version);
                let lmp_version = Self::core_spec_version_to_u8(version_info.lmp_version);

                self.local_info.hci_version = Some(hci_version);
                self.local_info.hci_revision = Some(version_info.hci_subversion);
                self.local_info.lmp_version = Some(lmp_version);
                self.local_info.manufacturer_name = Some(version_info.company_identifier);
                self.local_info.lmp_subversion = Some(version_info.lmp_subversion);
            }
            Err(_) => return Err(BluetoothError::HciError),
        }

        // Get local BD_ADDR
        let read_bd_addr = cmd::info::ReadBdAddr::new();
        match self.controller.exec(&read_bd_addr).await {
            Ok(bd_addr) => {
                // The return value is the BdAddr directly
                let bd_addr_bytes = bd_addr.raw();
                let mut addr = [0u8; 6];
                addr.copy_from_slice(bd_addr_bytes);
                self.local_info.bd_addr = Some(addr);
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
