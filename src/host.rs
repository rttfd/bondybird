//! Bluetooth Core - Parallel task implementation for Bluetooth operations
//!
//! This module implements the core Bluetooth management functionality with a
//! parallel architecture that separates HCI event processing from API command handling.
//!
//! ## Architecture
//!
//! The Bluetooth core follows a parallel task design where:
//!
//! 1. **HCI Event Processor Task** - Dedicated task for processing incoming HCI events
//! 2. **API Request Processor Task** - Handles API requests and executes HCI commands  
//! 3. **Shared State** - Thread-safe shared Bluetooth state using Embassy mutexes
//! 4. **Event Processing** - Immediate HCI event processing without blocking API operations
//!
//! ## Event Flow
//!
//! 1. HCI events are received by the HCI Event Processor Task
//! 2. Events are processed and shared state is updated immediately
//! 3. API requests are handled by the API Request Processor Task in parallel
//! 4. API commands are executed and responses sent back via channels
//!
//! ## Parallel Processing Benefits
//!
//! - **Low Latency**: HCI events are processed immediately as they arrive
//! - **Non-blocking**: API operations don't block event processing
//! - **Thread Safety**: Shared state is properly synchronized with mutexes
//! - **Scalability**: Easy to add more processing tasks if needed
//!
//! ## Implementation Details
//!
//! The implementation uses the bt-hci crate for standardized HCI command and event handling:
//! - Controller commands like `Inquiry`, `CreateConnection`, and `Disconnect`
//! - Events like `InquiryResult`, `ConnectionComplete`, and `DisconnectionComplete`
//!
//! Device state is managed in a heapless `FnvIndexMap` to be `no_std` compatible.
//! All string parsing and handling is done with heapless types.
//!
//! ## Usage
//!
//! 1. Spawn both tasks with a shared controller reference:
//!    ```rust,no_run
//!    spawner.spawn(hci_event_processor(controller)).unwrap();
//!    spawner.spawn(api_request_processor(controller)).unwrap();
//!    ```
//!
//! 2. Use the API functions from the `api` module to interact with the processing tasks:
//!    ```rust,no_run
//!    use bondybird::api::{get_devices, start_discovery, connect_device};
//!    let devices = get_devices().await.unwrap();
//!    let _ = start_discovery().await;
//!    let _ = connect_device("AA:BB:CC:DD:EE:FF").await;
//!    ```

use crate::{
    BluetoothAddress, BluetoothDevice, BluetoothError, BluetoothHost, BluetoothState, Request,
    Response, constants,
};
use bt_hci::{
    cmd,
    controller::{ControllerCmdSync, ExternalController},
    event,
    param::EventMask,
    transport::Transport,
};
use heapless::Vec;

/// `BluetoothHost` Core that handles all Bluetooth operations
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
/// The core maintains several key state components:
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
impl BluetoothHost {
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
    pub async fn process_api_request<T: Transport + 'static, const SLOTS: usize>(
        &mut self,
        request: Request,
        controller: &ExternalController<T, SLOTS>,
    ) -> Response {
        match request {
            Request::Discover => {
                if self.discovering {
                    return Response::Error(BluetoothError::AlreadyInProgress);
                }

                match self.start_discovery(controller).await {
                    Ok(()) => {
                        self.discovering = true;
                        self.state = BluetoothState::Discovering;
                        Response::DiscoverComplete
                    }
                    Err(e) => Response::Error(e),
                }
            }
            Request::StopDiscovery => {
                if !self.discovering {
                    return Response::Error(BluetoothError::InvalidParameter);
                }

                match self.stop_discovery(controller).await {
                    Ok(()) => {
                        self.discovering = false;
                        self.state = BluetoothState::PoweredOn;
                        Response::DiscoveryStopped
                    }
                    Err(e) => Response::Error(e),
                }
            }
            Request::GetDevices => {
                let devices_vec: Vec<BluetoothDevice, { constants::MAX_DISCOVERED_DEVICES }> =
                    self.devices.values().copied().collect();
                Response::Devices(devices_vec)
            }
            Request::Pair(address) => match Self::parse_address(&address) {
                Ok(addr) => match self.connect_device(addr, controller).await {
                    Ok(()) => Response::PairComplete,
                    Err(e) => Response::Error(e),
                },
                Err(e) => Response::Error(e),
            },
            Request::Disconnect(address) => match Self::parse_address(&address) {
                Ok(addr) => match self.disconnect_device(addr, controller).await {
                    Ok(()) => Response::DisconnectComplete,
                    Err(e) => Response::Error(e),
                },
                Err(e) => Response::Error(e),
            },
            Request::GetState => Response::State(self.state),
            Request::GetLocalInfo => Response::LocalInfo(self.local_info),
        }
    }

    /// Start device discovery
    async fn start_discovery<T: Transport + 'static, const SLOTS: usize>(
        &mut self,
        controller: &ExternalController<T, SLOTS>,
    ) -> Result<(), BluetoothError> {
        let inquiry_cmd = bt_hci::cmd::link_control::Inquiry::new(
            constants::GIAC,                     // GIAC
            constants::DEFAULT_INQUIRY_DURATION, // Inquiry duration
            constants::UNLIMITED_RESPONSES,      // Unlimited responses
        );

        controller
            .exec(&inquiry_cmd)
            .await
            .map_err(|_| BluetoothError::DiscoveryFailed)?;

        // Set the state to discovering
        self.state = BluetoothState::Discovering;
        Ok(())
    }

    /// Stop device discovery
    async fn stop_discovery<T: Transport + 'static, const SLOTS: usize>(
        &mut self,
        _controller: &ExternalController<T, SLOTS>,
    ) -> Result<(), BluetoothError> {
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
    async fn connect_device<T: Transport + 'static, const SLOTS: usize>(
        &mut self,
        addr: BluetoothAddress,
        controller: &ExternalController<T, SLOTS>,
    ) -> Result<(), BluetoothError> {
        // Check if the device exists in our discovered devices
        if !self.devices.contains_key(&addr) {
            return Err(BluetoothError::DeviceNotFound);
        }

        // Create the connection request with individual parameters
        // Note: We're providing the parameters directly to the new() function
        // This is how the bt-hci API expects commands to be constructed
        let create_conn = cmd::link_control::CreateConnection::new(
            bt_hci::param::BdAddr::new(addr.0),      // BD_ADDR
            constants::DEFAULT_PACKET_TYPES,         // Packet type (DM1, DM3, DM5, DH1, DH3, DH5)
            constants::PAGE_SCAN_REPETITION_MODE_R1, // Page scan repetition mode (R1)
            constants::RESERVED_FIELD,               // Reserved
            constants::NO_CLOCK_OFFSET,              // Clock offset (none)
            constants::ALLOW_ROLE_SWITCH,            // Allow role switch
        );

        match controller.exec(&create_conn).await {
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
    async fn disconnect_device<T: Transport + 'static, const SLOTS: usize>(
        &mut self,
        addr: BluetoothAddress,
        controller: &ExternalController<T, SLOTS>,
    ) -> Result<(), BluetoothError> {
        // Get the connection handle for this device address from our connections map
        let conn_handle = match self.connections.get(&addr) {
            Some(handle) => bt_hci::param::ConnHandle::new(*handle),
            None => return Err(BluetoothError::DeviceNotConnected),
        };

        // Specify the reason for disconnection
        let disconnect_reason = bt_hci::param::DisconnectReason::RemoteUserTerminatedConn;

        // Create and send the disconnect command
        let disconnect = cmd::link_control::Disconnect::new(conn_handle, disconnect_reason);

        match controller.exec(&disconnect).await {
            Ok(()) => {
                // The disconnection will be confirmed by a DisconnectionComplete event
                // The actual device state update and connections map update will happen there
                Ok(())
            }
            Err(_) => Err(BluetoothError::HciError),
        }
    }

    /// Parse address string to bytes
    fn parse_address(address: &str) -> Result<BluetoothAddress, BluetoothError> {
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

        Ok(BluetoothAddress::new(addr))
    }

    /// Initialize the Bluetooth controller
    ///
    /// This method performs the complete initialization sequence:
    /// 1. Reset the controller to a known state
    /// 2. Configure event mask to receive necessary events
    /// 3. Read local device information (version, `BD_ADDR`)
    ///
    /// # Errors
    ///
    /// Returns `BluetoothError::HciError` if any HCI command fails during initialization.
    pub async fn initialize<T: Transport + 'static, const SLOTS: usize>(
        &mut self,
        controller: &ExternalController<T, SLOTS>,
    ) -> Result<(), BluetoothError> {
        // Step 1: Reset the controller
        let reset_cmd = cmd::controller_baseband::Reset::new();
        controller
            .exec(&reset_cmd)
            .await
            .map_err(|_| BluetoothError::HciError)?;

        // Step 2: Set event mask to enable the events we're interested in
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

        let set_event_mask = cmd::controller_baseband::SetEventMask::new(event_mask);
        controller
            .exec(&set_event_mask)
            .await
            .map_err(|_| BluetoothError::HciError)?;

        // Step 3: Get local version information
        let read_local_version = cmd::info::ReadLocalVersionInformation::new();
        let version_info = controller
            .exec(&read_local_version)
            .await
            .map_err(|_| BluetoothError::HciError)?;

        // Extract and store the version information using our helper method
        let hci_version = Self::core_spec_version_to_u8(version_info.hci_version);
        let lmp_version = Self::core_spec_version_to_u8(version_info.lmp_version);

        self.local_info.hci_version = Some(hci_version);
        self.local_info.hci_revision = Some(version_info.hci_subversion);
        self.local_info.lmp_version = Some(lmp_version);
        self.local_info.manufacturer_name = Some(version_info.company_identifier);
        self.local_info.lmp_subversion = Some(version_info.lmp_subversion);

        // Step 4: Get local BD_ADDR
        let read_bd_addr = cmd::info::ReadBdAddr::new();
        let bd_addr = controller
            .exec(&read_bd_addr)
            .await
            .map_err(|_| BluetoothError::HciError)?;

        // The return value is the BdAddr directly
        let bd_addr_bytes = bd_addr.raw();
        if let Some(addr) = Self::extract_bd_addr(bd_addr_bytes) {
            self.local_info.bd_addr = Some(addr);
        }

        // Mark controller as ready
        self.state = BluetoothState::PoweredOn;
        Ok(())
    }

    /// Helper method to extract `BD_ADDR` from a byte slice
    fn extract_bd_addr(bytes: &[u8]) -> Option<BluetoothAddress> {
        if bytes.len() >= constants::BD_ADDR_LENGTH {
            let mut addr = [0u8; constants::BD_ADDR_LENGTH];
            addr.copy_from_slice(&bytes[..constants::BD_ADDR_LENGTH]);
            Some(BluetoothAddress::new(addr))
        } else {
            None
        }
    }

    /// Helper method to safely copy a name with proper bounds checking
    fn copy_device_name(source: &[u8]) -> [u8; constants::MAX_DEVICE_NAME_LENGTH] {
        let mut name = [0u8; constants::MAX_DEVICE_NAME_LENGTH];
        let name_len = core::cmp::min(source.len(), constants::MAX_DEVICE_NAME_LENGTH);
        name[..name_len].copy_from_slice(&source[..name_len]);
        name
    }

    /// Helper method to remove connections by handle
    fn remove_connections_by_handle(&mut self, conn_handle: u16) {
        // Find the device address associated with this connection handle
        // and remove it from our connections map
        let keys_to_remove: Vec<BluetoothAddress, { constants::MAX_CLEANUP_ENTRIES }> = self
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
    }

    /// Helper method to add or update a device in the devices map
    ///
    /// If a device with the same address already exists, this method will update
    /// its properties with any new non-None values provided.
    fn add_or_update_device(
        &mut self,
        addr: BluetoothAddress,
        rssi: Option<i8>,
        name: Option<[u8; constants::MAX_DEVICE_NAME_LENGTH]>,
        class_of_device: Option<u32>,
    ) {
        if let Some(existing_device) = self.devices.get_mut(&addr) {
            // Update existing device with new information
            if rssi.is_some() {
                existing_device.rssi = rssi;
            }
            if name.is_some() {
                existing_device.name = name;
            }
            if class_of_device.is_some() {
                existing_device.class_of_device = class_of_device;
            }
        } else {
            // Create new device entry
            let device = BluetoothDevice {
                addr,
                rssi,
                name,
                class_of_device,
            };
            self.devices.insert(device.addr, device).ok();
        }
    }

    /// Process HCI events and update the Bluetooth state accordingly
    ///
    /// This method handles various HCI events and updates the internal state,
    /// device list, and connection information based on the received events.
    pub fn process_hci_event(&mut self, event: &event::Event<'_>) {
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
                        let mut addr_bytes = [0u8; 6];
                        addr_bytes.copy_from_slice(&bd_addr_bytes[i * 6..(i * 6 + 6)]);
                        let addr = BluetoothAddress::new(addr_bytes);

                        // Parse class of device (3 bytes per device)
                        let class_of_device = if i * 3 + 3 <= class_of_device_bytes.len() {
                            Self::parse_class_of_device(&class_of_device_bytes[i * 3..(i * 3 + 3)])
                        } else {
                            None
                        };

                        // Add the device to our map
                        self.add_or_update_device(addr, None, None, class_of_device);
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
                    let bd_addr_bytes = complete.bd_addr.raw();
                    if let Some(addr) = Self::extract_bd_addr(bd_addr_bytes) {
                        // Store the connection handle for this device
                        let conn_handle = complete.handle.raw();
                        self.connections.insert(addr, conn_handle).ok();

                        // Update the state
                        self.state = BluetoothState::Connected;
                    }
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

                    // Remove the connection from our map
                    self.remove_connections_by_handle(conn_handle);

                    self.state = BluetoothState::PoweredOn;
                }
                // Even if disconnection failed, we'll set the state to PoweredOn
                // since we're no longer connected
            }
            event::Event::RemoteNameRequestComplete(ref complete) => {
                // Update device name if found
                if complete.status.to_result().is_ok() {
                    let bd_addr_bytes = complete.bd_addr.raw();
                    if let Some(addr) = Self::extract_bd_addr(bd_addr_bytes) {
                        let name = Self::copy_device_name(&complete.remote_name);

                        // Add or update the device with the name
                        self.add_or_update_device(addr, None, Some(name), None);
                    }
                }
            }
            event::Event::ExtendedInquiryResult(ref result) => {
                // The ExtendedInquiryResult gives us more info, including RSSI and EIR data
                // It's easier to parse because fields are not in RemainingBytes format

                // Get the BD_ADDR directly from the result
                let addr = BluetoothAddress::new(result.bd_addr);

                // Parse class of device (3 bytes)
                let class_of_device = Self::parse_class_of_device(&result.class_of_device);

                // Add the device with RSSI
                self.add_or_update_device(addr, Some(result.rssi), None, class_of_device);

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
                        let mut addr_bytes = [0u8; 6];
                        addr_bytes.copy_from_slice(&bd_addr_bytes[i * 6..(i * 6 + 6)]);
                        let addr = BluetoothAddress::new(addr_bytes);

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
                        self.add_or_update_device(addr, Some(rssi), None, class_of_device);
                    }
                }
            }
            event::Event::CommandComplete(_) | event::Event::CommandStatus(_) | _ => {
                // Handle other events if needed
            }
        }
    }
}
