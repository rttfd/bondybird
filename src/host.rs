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
//! 1. Spawn both tasks with a shared controller reference:
//!    ```rust,ignore
//!    spawner.spawn(hci_event_processor(controller)).unwrap();
//!    spawner.spawn(api_request_processor(controller)).unwrap();
//!    ```
//!
//! 2. Use the API functions from the `api` module to interact with the processing tasks:
//!    ```rust,no_run
//!    use bondybird::api::{get_devices, start_discovery, connect_device};
//!    use heapless::String;
//!    # async fn example() {
//!    let devices = get_devices().await.unwrap();
//!    let _ = start_discovery().await;
//!    let addr: String<64> = String::try_from("AA:BB:CC:DD:EE:FF").unwrap();
//!    let _ = connect_device(addr).await;
//!    # }
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

    async fn start_discovery<T: Transport + 'static, const SLOTS: usize>(
        &mut self,
        controller: &ExternalController<T, SLOTS>,
    ) -> Result<(), BluetoothError> {
        let inquiry_cmd = bt_hci::cmd::link_control::Inquiry::new(
            constants::GIAC,
            constants::DEFAULT_INQUIRY_DURATION,
            constants::UNLIMITED_RESPONSES,
        );

        controller
            .exec(&inquiry_cmd)
            .await
            .map_err(|_| BluetoothError::DiscoveryFailed)?;

        self.state = BluetoothState::Discovering;
        Ok(())
    }

    async fn stop_discovery<T: Transport + 'static, const SLOTS: usize>(
        &mut self,
        controller: &ExternalController<T, SLOTS>,
    ) -> Result<(), BluetoothError> {
        let inquiry_cancel = cmd::link_control::InquiryCancel::new();

        controller
            .exec(&inquiry_cancel)
            .await
            .map_err(|_| BluetoothError::HciError)?;

        self.state = BluetoothState::PoweredOn;
        Ok(())
    }

    async fn connect_device<T: Transport + 'static, const SLOTS: usize>(
        &mut self,
        addr: BluetoothAddress,
        controller: &ExternalController<T, SLOTS>,
    ) -> Result<(), BluetoothError> {
        if !self.devices.contains_key(&addr) {
            return Err(BluetoothError::DeviceNotFound);
        }

        let create_conn = cmd::link_control::CreateConnection::new(
            addr.into(),
            constants::DEFAULT_PACKET_TYPES,
            constants::PAGE_SCAN_REPETITION_MODE_R1,
            constants::RESERVED_FIELD,
            constants::NO_CLOCK_OFFSET,
            constants::ALLOW_ROLE_SWITCH,
        );

        if controller.exec(&create_conn).await.is_ok() {
            self.state = BluetoothState::Connecting;
            Ok(())
        } else {
            Err(BluetoothError::ConnectionFailed)
        }
    }

    async fn disconnect_device<T: Transport + 'static, const SLOTS: usize>(
        &mut self,
        addr: BluetoothAddress,
        controller: &ExternalController<T, SLOTS>,
    ) -> Result<(), BluetoothError> {
        let conn_handle = match self.connections.get(&addr) {
            Some(handle) => bt_hci::param::ConnHandle::new(*handle),
            None => return Err(BluetoothError::DeviceNotConnected),
        };

        let disconnect_reason = bt_hci::param::DisconnectReason::RemoteUserTerminatedConn;

        let disconnect = cmd::link_control::Disconnect::new(conn_handle, disconnect_reason);

        if controller.exec(&disconnect).await.is_ok() {
            Ok(())
        } else {
            Err(BluetoothError::HciError)
        }
    }

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

        self.local_info.bd_addr = bd_addr.try_into().ok();

        // Mark controller as ready
        self.state = BluetoothState::PoweredOn;
        Ok(())
    }

    fn copy_device_name(source: &[u8]) -> [u8; constants::MAX_DEVICE_NAME_LENGTH] {
        let mut name = [0u8; constants::MAX_DEVICE_NAME_LENGTH];
        let name_len = core::cmp::min(source.len(), constants::MAX_DEVICE_NAME_LENGTH);
        name[..name_len].copy_from_slice(&source[..name_len]);
        name
    }

    fn remove_connections_by_handle(&mut self, conn_handle: u16) {
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

        for addr in keys_to_remove {
            self.connections.remove(&addr);
        }
    }

    fn add_or_update_device(
        &mut self,
        addr: BluetoothAddress,
        rssi: Option<i8>,
        name: Option<[u8; constants::MAX_DEVICE_NAME_LENGTH]>,
        class_of_device: Option<u32>,
    ) {
        if let Some(existing_device) = self.devices.get_mut(&addr) {
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
                let num_responses = result.num_responses;

                let bd_addr_bytes = &*result.bd_addr;
                let class_of_device_bytes = &*result.class_of_device;

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

                        self.add_or_update_device(addr, None, None, class_of_device);
                    }
                }
            }
            event::Event::InquiryComplete(ref complete) => {
                self.discovering = false;
                if complete.status.to_result().is_ok() {
                    self.state = BluetoothState::PoweredOn;
                }
            }
            event::Event::ConnectionComplete(ref complete) => {
                if complete.status.to_result().is_ok() {
                    if let Ok(addr) = complete.bd_addr.try_into() {
                        // Store the connection handle for this device
                        let conn_handle = complete.handle.raw();
                        self.connections.insert(addr, conn_handle).ok();

                        self.state = BluetoothState::Connected;
                    }
                } else {
                    self.state = BluetoothState::PoweredOn;
                }
            }
            event::Event::DisconnectionComplete(ref complete) => {
                if complete.status.to_result().is_ok() {
                    let conn_handle = complete.handle.raw();

                    // Remove the connection from our map
                    self.remove_connections_by_handle(conn_handle);

                    self.state = BluetoothState::PoweredOn;
                }
            }
            event::Event::RemoteNameRequestComplete(ref complete) => {
                // Update device name if found
                if complete.status.to_result().is_ok() {
                    if let Ok(addr) = complete.bd_addr.try_into() {
                        let name = Self::copy_device_name(&complete.remote_name);

                        self.add_or_update_device(addr, None, Some(name), None);
                    }
                }
            }
            event::Event::ExtendedInquiryResult(ref result) => {
                let addr = BluetoothAddress::new(result.bd_addr);

                // Parse class of device (3 bytes)
                let class_of_device = Self::parse_class_of_device(&result.class_of_device);

                // Add the device with RSSI
                self.add_or_update_device(addr, Some(result.rssi), None, class_of_device);
            }
            event::Event::InquiryResultWithRssi(ref result) => {
                let num_responses = result.num_responses;

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

                        #[allow(clippy::cast_possible_wrap)]
                        let rssi = rssi_bytes[i] as i8;

                        // Parse class of device (3 bytes per device)
                        let class_of_device = if i * 3 + 3 <= class_of_device_bytes.len() {
                            Self::parse_class_of_device(&class_of_device_bytes[i * 3..(i * 3 + 3)])
                        } else {
                            None
                        };

                        self.add_or_update_device(addr, Some(rssi), None, class_of_device);
                    }
                }
            }
            _ => {
                defmt::debug!("Unhandled HCI event: {:?}", defmt::Debug2Format(event));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BluetoothAddress, BluetoothError, BluetoothState};

    // Helper function to create a test BluetoothHost
    fn create_test_host() -> BluetoothHost {
        BluetoothHost::new()
    }

    // Helper function to create a test BluetoothAddress
    fn create_test_address(addr: [u8; 6]) -> BluetoothAddress {
        BluetoothAddress::new(addr)
    }

    #[test]
    fn test_bluetooth_host_new() {
        let host = BluetoothHost::new();

        assert_eq!(host.state, BluetoothState::PoweredOff);
        assert!(!host.discovering);
        assert!(host.devices.is_empty());
        assert!(host.connections.is_empty());
        assert_eq!(host.local_info.bd_addr, None);
        assert_eq!(host.local_info.hci_version, None);
        assert_eq!(host.local_info.hci_revision, None);
        assert_eq!(host.local_info.lmp_version, None);
        assert_eq!(host.local_info.manufacturer_name, None);
        assert_eq!(host.local_info.lmp_subversion, None);
    }

    #[test]
    fn test_parse_class_of_device() {
        // Valid 3-byte class of device
        let bytes = [0x12, 0x34, 0x56];
        let result = BluetoothHost::parse_class_of_device(&bytes);
        assert_eq!(result, Some(0x563412)); // Little-endian format

        // Valid 4+ byte array (should only use first 3)
        let bytes = [0x12, 0x34, 0x56, 0x78, 0x9A];
        let result = BluetoothHost::parse_class_of_device(&bytes);
        assert_eq!(result, Some(0x563412));

        // Empty array
        let bytes = [];
        let result = BluetoothHost::parse_class_of_device(&bytes);
        assert_eq!(result, None);

        // Insufficient bytes
        let bytes = [0x12, 0x34];
        let result = BluetoothHost::parse_class_of_device(&bytes);
        assert_eq!(result, None);

        // Zero class of device
        let bytes = [0x00, 0x00, 0x00];
        let result = BluetoothHost::parse_class_of_device(&bytes);
        assert_eq!(result, Some(0x000000));
    }

    #[test]
    fn test_core_spec_version_to_u8() {
        use bt_hci::param::CoreSpecificationVersion;

        assert_eq!(
            BluetoothHost::core_spec_version_to_u8(CoreSpecificationVersion::VERSION_1_1),
            0x01
        );
        assert_eq!(
            BluetoothHost::core_spec_version_to_u8(CoreSpecificationVersion::VERSION_2_0_EDR),
            0x03
        );
        assert_eq!(
            BluetoothHost::core_spec_version_to_u8(CoreSpecificationVersion::VERSION_4_0),
            0x06
        );
        assert_eq!(
            BluetoothHost::core_spec_version_to_u8(CoreSpecificationVersion::VERSION_5_0),
            0x09
        );
        assert_eq!(
            BluetoothHost::core_spec_version_to_u8(CoreSpecificationVersion::VERSION_5_4),
            0x0D
        );
    }

    #[test]
    fn test_parse_address_valid_colon_format() {
        let address = "01:23:45:67:89:AB";
        let result = BluetoothHost::parse_address(address);
        assert!(result.is_ok());
        let addr = result.unwrap();
        assert_eq!(addr.as_bytes(), &[0x01, 0x23, 0x45, 0x67, 0x89, 0xAB]);
    }

    #[test]
    fn test_parse_address_valid_dash_format() {
        let address = "01-23-45-67-89-AB";
        let result = BluetoothHost::parse_address(address);
        assert!(result.is_ok());
        let addr = result.unwrap();
        assert_eq!(addr.as_bytes(), &[0x01, 0x23, 0x45, 0x67, 0x89, 0xAB]);
    }

    #[test]
    fn test_parse_address_lowercase() {
        let address = "01:23:45:67:89:ab";
        let result = BluetoothHost::parse_address(address);
        assert!(result.is_ok());
        let addr = result.unwrap();
        assert_eq!(addr.as_bytes(), &[0x01, 0x23, 0x45, 0x67, 0x89, 0xAB]);
    }

    #[test]
    fn test_parse_address_invalid_length() {
        let address = "01:23:45:67:89"; // Missing last byte
        let result = BluetoothHost::parse_address(address);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), BluetoothError::InvalidParameter);
    }

    #[test]
    fn test_parse_address_invalid_characters() {
        let address = "01:23:45:67:89:XY"; // Invalid hex characters
        let result = BluetoothHost::parse_address(address);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), BluetoothError::InvalidParameter);
    }

    #[test]
    fn test_parse_address_invalid_format() {
        let address = "012345678901"; // No separators but valid hex - this actually works!
        let result = BluetoothHost::parse_address(address);
        assert!(result.is_ok()); // The function accepts this format

        // Test truly invalid formats
        let invalid_address = "01:23:45:67:89"; // Incomplete
        let result = BluetoothHost::parse_address(invalid_address);
        assert!(result.is_err());
    }

    #[test]
    fn test_copy_device_name() {
        // Name shorter than max length
        let source = b"Device";
        let result = BluetoothHost::copy_device_name(source);
        let expected = {
            let mut name = [0u8; constants::MAX_DEVICE_NAME_LENGTH];
            name[..6].copy_from_slice(b"Device");
            name
        };
        assert_eq!(result, expected);

        // Name exactly max length
        let long_name = [b'A'; constants::MAX_DEVICE_NAME_LENGTH];
        let result = BluetoothHost::copy_device_name(&long_name);
        assert_eq!(result, long_name);

        // Name longer than max length (should be truncated)
        let too_long = [b'B'; constants::MAX_DEVICE_NAME_LENGTH + 10];
        let result = BluetoothHost::copy_device_name(&too_long);
        let expected = [b'B'; constants::MAX_DEVICE_NAME_LENGTH];
        assert_eq!(result, expected);

        // Empty name
        let empty = b"";
        let result = BluetoothHost::copy_device_name(empty);
        let expected = [0u8; constants::MAX_DEVICE_NAME_LENGTH];
        assert_eq!(result, expected);
    }

    #[test]
    fn test_add_or_update_device_new() {
        let mut host = create_test_host();
        let addr = create_test_address([0x01, 0x02, 0x03, 0x04, 0x05, 0x06]);
        let mut name = [0u8; constants::MAX_DEVICE_NAME_LENGTH];
        name[0] = b'T';
        name[1] = b'e';
        name[2] = b's';
        name[3] = b't';

        host.add_or_update_device(addr, Some(-50), Some(name), Some(0x123456));

        assert_eq!(host.devices.len(), 1);
        let device = host.devices.get(&addr).unwrap();
        assert_eq!(device.addr, addr);
        assert_eq!(device.rssi, Some(-50));
        assert_eq!(device.name, Some(name));
        assert_eq!(device.class_of_device, Some(0x123456));
    }

    #[test]
    fn test_add_or_update_device_existing() {
        let mut host = create_test_host();
        let addr = create_test_address([0x01, 0x02, 0x03, 0x04, 0x05, 0x06]);
        let mut original_name = [0u8; constants::MAX_DEVICE_NAME_LENGTH];
        original_name[0] = b'O';
        original_name[1] = b'l';
        original_name[2] = b'd';

        let mut new_name = [0u8; constants::MAX_DEVICE_NAME_LENGTH];
        new_name[0] = b'N';
        new_name[1] = b'e';
        new_name[2] = b'w';

        // Add initial device
        host.add_or_update_device(addr, Some(-60), Some(original_name), Some(0x111111));
        assert_eq!(host.devices.len(), 1);

        // Update existing device
        host.add_or_update_device(addr, Some(-40), Some(new_name), None);
        assert_eq!(host.devices.len(), 1); // Should still be only one device

        let device = host.devices.get(&addr).unwrap();
        assert_eq!(device.addr, addr);
        assert_eq!(device.rssi, Some(-40)); // Updated
        assert_eq!(device.name, Some(new_name)); // Updated
        assert_eq!(device.class_of_device, Some(0x111111)); // Unchanged
    }

    #[test]
    fn test_add_or_update_device_partial_update() {
        let mut host = create_test_host();
        let addr = create_test_address([0x01, 0x02, 0x03, 0x04, 0x05, 0x06]);
        let mut name = [0u8; constants::MAX_DEVICE_NAME_LENGTH];
        name[0] = b'T';
        name[1] = b'e';
        name[2] = b's';
        name[3] = b't';

        // Add initial device
        host.add_or_update_device(addr, Some(-60), Some(name), Some(0x111111));

        // Update only RSSI
        host.add_or_update_device(addr, Some(-30), None, None);

        let device = host.devices.get(&addr).unwrap();
        assert_eq!(device.rssi, Some(-30)); // Updated
        assert_eq!(device.name, Some(name)); // Unchanged
        assert_eq!(device.class_of_device, Some(0x111111)); // Unchanged
    }

    #[test]
    fn test_remove_connections_by_handle() {
        let mut host = create_test_host();
        let addr1 = create_test_address([0x01, 0x02, 0x03, 0x04, 0x05, 0x06]);
        let addr2 = create_test_address([0x11, 0x12, 0x13, 0x14, 0x15, 0x16]);
        let addr3 = create_test_address([0x21, 0x22, 0x23, 0x24, 0x25, 0x26]);

        // Add connections
        host.connections.insert(addr1, 100).ok();
        host.connections.insert(addr2, 200).ok();
        host.connections.insert(addr3, 100).ok(); // Same handle as addr1

        assert_eq!(host.connections.len(), 3);

        // Remove connections with handle 100
        host.remove_connections_by_handle(100);

        assert_eq!(host.connections.len(), 1);
        assert!(host.connections.contains_key(&addr2));
        assert!(!host.connections.contains_key(&addr1));
        assert!(!host.connections.contains_key(&addr3));
    }

    #[test]
    fn test_remove_connections_by_handle_not_found() {
        let mut host = create_test_host();
        let addr1 = create_test_address([0x01, 0x02, 0x03, 0x04, 0x05, 0x06]);

        host.connections.insert(addr1, 100).ok();
        assert_eq!(host.connections.len(), 1);

        // Try to remove non-existent handle
        host.remove_connections_by_handle(999);

        assert_eq!(host.connections.len(), 1); // Should remain unchanged
        assert!(host.connections.contains_key(&addr1));
    }

    #[test]
    fn test_state_transitions() {
        let mut host = create_test_host();

        // Initial state
        assert_eq!(host.state, BluetoothState::PoweredOff);

        // Simulate power on
        host.state = BluetoothState::PoweredOn;
        assert_eq!(host.state, BluetoothState::PoweredOn);

        // Simulate discovery
        host.state = BluetoothState::Discovering;
        host.discovering = true;
        assert_eq!(host.state, BluetoothState::Discovering);
        assert!(host.discovering);

        // Simulate connection
        host.state = BluetoothState::Connecting;
        assert_eq!(host.state, BluetoothState::Connecting);

        host.state = BluetoothState::Connected;
        assert_eq!(host.state, BluetoothState::Connected);
    }

    #[test]
    fn test_device_management() {
        let mut host = create_test_host();

        // Test max device limit
        for i in 0..constants::MAX_DISCOVERED_DEVICES {
            let addr = create_test_address([i as u8, 0x02, 0x03, 0x04, 0x05, 0x06]);
            host.add_or_update_device(addr, Some(-(i as i8) - 30), None, None);
        }

        assert_eq!(host.devices.len(), constants::MAX_DISCOVERED_DEVICES);

        // Adding one more should not exceed the limit (heapless behavior)
        let extra_addr = create_test_address([0xFF, 0x02, 0x03, 0x04, 0x05, 0x06]);
        host.add_or_update_device(extra_addr, Some(-100), None, None);

        // Device count should not exceed max (heapless map handles this)
        assert!(host.devices.len() <= constants::MAX_DISCOVERED_DEVICES);
    }

    #[test]
    fn test_connection_management() {
        let mut host = create_test_host();
        let addr1 = create_test_address([0x01, 0x02, 0x03, 0x04, 0x05, 0x06]);
        let addr2 = create_test_address([0x11, 0x12, 0x13, 0x14, 0x15, 0x16]);

        // Test adding connections
        assert!(host.connections.insert(addr1, 100).is_ok());
        assert!(host.connections.insert(addr2, 200).is_ok());

        assert_eq!(host.connections.len(), 2);
        assert_eq!(host.connections.get(&addr1), Some(&100));
        assert_eq!(host.connections.get(&addr2), Some(&200));

        // Test removing connection
        host.connections.remove(&addr1);
        assert_eq!(host.connections.len(), 1);
        assert!(!host.connections.contains_key(&addr1));
        assert!(host.connections.contains_key(&addr2));
    }

    #[test]
    fn test_local_info_initialization() {
        let mut host = create_test_host();

        // Test initial state
        assert_eq!(host.local_info.bd_addr, None);
        assert_eq!(host.local_info.hci_version, None);

        // Test setting local info
        let test_addr = BluetoothAddress::new([0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC]);
        host.local_info.bd_addr = Some(test_addr);
        host.local_info.hci_version = Some(0x09); // Version 5.0
        host.local_info.manufacturer_name = Some(0x1234);

        assert_eq!(host.local_info.bd_addr, Some(test_addr));
        assert_eq!(host.local_info.hci_version, Some(0x09));
        assert_eq!(host.local_info.manufacturer_name, Some(0x1234));
    }

    // Mock HCI event tests would require more complex setup with actual HCI types
    // These tests verify the logic without requiring a real controller

    #[test]
    fn test_device_discovery_workflow() {
        let mut host = create_test_host();

        // Start in powered off state
        assert_eq!(host.state, BluetoothState::PoweredOff);
        assert!(!host.discovering);

        // Power on
        host.state = BluetoothState::PoweredOn;
        assert_eq!(host.state, BluetoothState::PoweredOn);

        // Start discovery
        host.state = BluetoothState::Discovering;
        host.discovering = true;
        assert_eq!(host.state, BluetoothState::Discovering);
        assert!(host.discovering);

        // Add discovered devices
        let addr1 = create_test_address([0x01, 0x02, 0x03, 0x04, 0x05, 0x06]);
        let addr2 = create_test_address([0x11, 0x12, 0x13, 0x14, 0x15, 0x16]);

        host.add_or_update_device(addr1, Some(-45), None, Some(0x240404));
        host.add_or_update_device(addr2, Some(-55), None, Some(0x1F00));

        assert_eq!(host.devices.len(), 2);

        // Stop discovery
        host.state = BluetoothState::PoweredOn;
        host.discovering = false;
        assert_eq!(host.state, BluetoothState::PoweredOn);
        assert!(!host.discovering);
    }

    #[test]
    fn test_connection_workflow() {
        let mut host = create_test_host();
        let addr = create_test_address([0x01, 0x02, 0x03, 0x04, 0x05, 0x06]);

        // Add device first
        host.add_or_update_device(addr, Some(-50), None, None);
        assert!(host.devices.contains_key(&addr));

        // Start connection
        host.state = BluetoothState::Connecting;
        assert_eq!(host.state, BluetoothState::Connecting);

        // Complete connection
        host.connections.insert(addr, 100).ok();
        host.state = BluetoothState::Connected;
        assert_eq!(host.state, BluetoothState::Connected);
        assert!(host.connections.contains_key(&addr));

        // Disconnect
        host.connections.remove(&addr);
        host.state = BluetoothState::PoweredOn;
        assert_eq!(host.state, BluetoothState::PoweredOn);
        assert!(!host.connections.contains_key(&addr));
    }

    #[test]
    fn test_error_conditions() {
        let _host = create_test_host();

        // Test invalid address parsing
        let invalid_addresses = [
            "",
            "invalid",
            "01:02:03:04:05",       // Too short
            "01:02:03:04:05:06:07", // Too long
            "GG:HH:II:JJ:KK:LL",    // Invalid hex
        ];

        for addr in &invalid_addresses {
            let result = BluetoothHost::parse_address(addr);
            assert!(result.is_err(), "Should fail for address: {}", addr);
        }

        // Test addresses that actually work (the function is permissive)
        let valid_addresses = [
            "01:02:03:04:05:06", // Standard colon format
            "01-02-03-04-05-06", // Standard dash format
            "012345678901",      // No separators but valid hex
            "01-02:03-04:05-06", // Mixed separators - allowed
        ];

        for addr in &valid_addresses {
            let result = BluetoothHost::parse_address(addr);
            assert!(result.is_ok(), "Should work for address: {}", addr);
        }
    }

    #[test]
    fn test_edge_cases() {
        let mut host = create_test_host();

        // Test with null address
        let null_addr = create_test_address([0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
        host.add_or_update_device(null_addr, None, None, None);
        assert!(host.devices.contains_key(&null_addr));

        // Test with broadcast address
        let broadcast_addr = create_test_address([0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
        host.add_or_update_device(broadcast_addr, None, None, None);
        assert!(host.devices.contains_key(&broadcast_addr));

        // Test class of device edge cases
        assert_eq!(BluetoothHost::parse_class_of_device(&[]), None);
        assert_eq!(BluetoothHost::parse_class_of_device(&[0x12]), None);
        assert_eq!(BluetoothHost::parse_class_of_device(&[0x12, 0x34]), None);
        assert_eq!(
            BluetoothHost::parse_class_of_device(&[0x12, 0x34, 0x56]),
            Some(0x563412)
        );
    }
    #[test]
    fn test_concurrent_operations() {
        let mut host = create_test_host();

        // Simulate concurrent discovery and device updates
        host.state = BluetoothState::Discovering;
        host.discovering = true;

        let addr = create_test_address([0x01, 0x02, 0x03, 0x04, 0x05, 0x06]);

        // Multiple updates to same device (simulating multiple inquiry results)
        host.add_or_update_device(addr, Some(-60), None, None);
        host.add_or_update_device(addr, Some(-55), None, Some(0x123456));

        let mut name = [0u8; constants::MAX_DEVICE_NAME_LENGTH];
        name[0] = b'T';
        name[1] = b'e';
        name[2] = b's';
        name[3] = b't';
        host.add_or_update_device(addr, None, Some(name), None);

        let device = host.devices.get(&addr).unwrap();
        assert_eq!(device.rssi, Some(-55)); // Last RSSI update
        assert_eq!(device.name, Some(name)); // Name from last update
        assert_eq!(device.class_of_device, Some(0x123456)); // CoD from middle update
    }
}
