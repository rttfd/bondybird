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
//! 4. Execute API commands and responses sent back via channels
//!
//! ## Parallel Processing Benefits
//!
//! - **Low Latency**: process HCI events immediately as they arrive
//! - **Non-blocking**: API operations don't block event processing
//! - **Thread Safety**: properly synchronize shared state with mutexes
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
use bt_hci::param::{AllowRoleSwitch, ClockOffset, PacketType, PageScanRepetitionMode};
use bt_hci::{
    cmd,
    controller::{ControllerCmdSync, ExternalController},
    event,
    param::EventMask,
    transport::Transport,
};
use heapless::{String, Vec};

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
                        Response::DiscoverStarted
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
                    self.devices.values().cloned().collect();
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
            Request::GetPairedDevices => {
                // Get devices that have active connections (paired/connected devices)
                let mut paired_devices: Vec<BluetoothDevice, { constants::MAX_CHANNELS }> =
                    Vec::new();

                for (addr, _handle) in &self.connections {
                    if let Some(device) = self.devices.get(addr) {
                        if paired_devices.push(device.clone()).is_err() {
                            // If we can't add more devices, break (shouldn't happen with proper constants)
                            break;
                        }
                    }
                }

                Response::PairedDevices(paired_devices)
            }
            Request::GetDeviceName(address) => match Self::parse_address(&address) {
                Ok(addr) => match self.get_device_name(addr, controller).await {
                    Ok(name) => Response::DeviceName(address, name),
                    Err(e) => Response::Error(e),
                },
                Err(e) => Response::Error(e),
            },
        }
    }

    async fn start_discovery<T: Transport + 'static, const SLOTS: usize>(
        &mut self,
        controller: &ExternalController<T, SLOTS>,
    ) -> Result<(), BluetoothError> {
        let options = self.options();
        let inquiry_cmd = bt_hci::cmd::link_control::Inquiry::new(
            options.lap,
            options.inquiry_length,
            options.num_responses,
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

        // DM1, DM3, DM5, DH1, DH3, DH5
        let packet_type = PacketType::new()
            .set_dh1_may_be_used(true)
            .set_dh3_may_be_used(true)
            .set_dh5_may_be_used(true)
            .set_dm3_may_be_used(true)
            .set_dm5_may_be_used(true);

        let create_conn = cmd::link_control::CreateConnection::new(
            addr.into(),
            packet_type,
            PageScanRepetitionMode::R1,
            constants::RESERVED_FIELD,
            ClockOffset::new(),
            AllowRoleSwitch::Allowed,
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

    async fn get_device_name<T: Transport + 'static, const SLOTS: usize>(
        &mut self,
        addr: BluetoothAddress,
        controller: &ExternalController<T, SLOTS>,
    ) -> Result<String<32>, BluetoothError> {
        // Use Remote Name Request to get the device name
        let remote_name_req = cmd::link_control::RemoteNameRequest::new(
            addr.into(),
            bt_hci::param::PageScanRepetitionMode::R1,
            constants::RESERVED_FIELD,
            bt_hci::param::ClockOffset::new(),
        );

        if controller.exec(&remote_name_req).await.is_ok() {
            // The response will come as a RemoteNameRequestComplete event
            // For now, check if we already have the name in our device cache
            if let Some(device) = self.devices.get(&addr) {
                if let Some(ref name) = device.name {
                    return Ok(name.clone());
                }
            }

            // If not found in cache, return a placeholder error
            // In a real implementation, we would wait for the RemoteNameRequestComplete event
            Err(BluetoothError::DeviceNotFound)
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
        defmt::debug!("Controller reset complete");

        // Step 2: Set event mask to enable the events we're interested in
        let event_mask = EventMask::new()
            .enable_inquiry_complete(true)
            .enable_inquiry_result(true)
            .enable_inquiry_result_with_rssi(true)
            .enable_ext_inquiry_result(true)
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
        defmt::debug!("Event mask set successfully");

        // Step 3: Get local version information
        let read_local_version = cmd::info::ReadLocalVersionInformation::new();
        let version_info = controller
            .exec(&read_local_version)
            .await
            .map_err(|_| BluetoothError::HciError)?;
        defmt::debug!(
            "Local version info: {:?}",
            defmt::Debug2Format(&version_info)
        );

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
        defmt::debug!("Local BD_ADDR: {:?}", defmt::Debug2Format(&bd_addr));

        self.local_info.bd_addr = bd_addr.try_into().ok();

        // Mark controller as ready
        self.state = BluetoothState::PoweredOn;

        defmt::debug!("Bluetooth controller initialized successfully");
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

    fn upsert_device(&mut self, device: BluetoothDevice) {
        if let Some(existing_device) = self.devices.get_mut(&device.addr) {
            *existing_device = device;
        } else {
            self.devices.insert(device.addr, device).ok();
        }
    }

    fn update_device_name(
        &mut self,
        addr: BluetoothAddress,
        name_bytes: [u8; constants::MAX_DEVICE_NAME_LENGTH],
    ) {
        // Use the shared utility function to convert bytes to string
        if let Some(name) = BluetoothDevice::bytes_to_name_string(&name_bytes) {
            if let Some(device) = self.devices.get_mut(&addr) {
                device.name = Some(name.clone());
            } else {
                // If the device doesn't exist, create a new entry
                let new_device = BluetoothDevice {
                    addr,
                    rssi: None,
                    name: Some(name),
                    class_of_device: None,
                };
                self.devices.insert(addr, new_device).ok();
            }
        }
    }

    /// Process HCI events and update the Bluetooth state accordingly
    ///
    /// This method handles various HCI events and updates the internal state,
    /// device list, and connection information based on the received events.
    pub fn process_hci_event(&mut self, event: &event::Event<'_>) {
        match *event {
            event::Event::InquiryResult(ref result) => {
                result.iter().for_each(|res| {
                    if let Ok(device) = res.try_into() {
                        self.upsert_device(device);
                    }
                });
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
                        self.update_device_name(addr, name);
                    }
                }
            }
            event::Event::ExtendedInquiryResult(ref result) => {
                if let Ok(device) = result.try_into() {
                    self.upsert_device(device);
                }
            }
            event::Event::InquiryResultWithRssi(ref result) => {
                result.iter().for_each(|res| {
                    if let Ok(device) = res.try_into() {
                        self.upsert_device(device);
                    }
                });
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
    use crate::{
        BluetoothAddress, BluetoothDevice, BluetoothError, BluetoothState, ClassOfDevice, constants,
    };

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
        let result = BluetoothDevice::parse_class_of_device(&bytes);
        assert_eq!(result, Some(ClassOfDevice::from_raw(0x563412))); // Little-endian format

        // Valid 4+ byte array (should only use first 3)
        let bytes = [0x12, 0x34, 0x56, 0x78, 0x9A];
        let result = BluetoothDevice::parse_class_of_device(&[bytes[0], bytes[1], bytes[2]]);
        assert_eq!(result, Some(ClassOfDevice::from_raw(0x563412)));

        // Zero class of device
        let bytes = [0x00, 0x00, 0x00];
        let result = BluetoothDevice::parse_class_of_device(&bytes);
        assert_eq!(result, None);
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
        let name = heapless::String::try_from("Test").unwrap();
        let device = BluetoothDevice {
            addr,
            rssi: Some(-50),
            name: Some(name.clone()),
            class_of_device: Some(ClassOfDevice::from_raw(0x123456)),
        };
        host.upsert_device(device);
        assert_eq!(host.devices.len(), 1);
        let device = host.devices.get(&addr).unwrap();
        assert_eq!(device.addr, addr);
        assert_eq!(device.rssi, Some(-50));
        assert_eq!(device.name, Some(name));
        assert_eq!(
            device.class_of_device,
            Some(ClassOfDevice::from_raw(0x123456))
        );
    }

    #[test]
    fn test_add_or_update_device_existing() {
        let mut host = create_test_host();
        let addr = create_test_address([0x01, 0x02, 0x03, 0x04, 0x05, 0x06]);
        let original_name = heapless::String::try_from("Old").unwrap();
        let new_name = heapless::String::try_from("New").unwrap();
        // Add initial device
        let device = BluetoothDevice {
            addr,
            rssi: Some(-60),
            name: Some(original_name),
            class_of_device: Some(ClassOfDevice::from_raw(0x111111)),
        };
        host.upsert_device(device);
        assert_eq!(host.devices.len(), 1);
        // Update existing device
        let device = BluetoothDevice {
            addr,
            rssi: Some(-40),
            name: Some(new_name.clone()),
            class_of_device: Some(ClassOfDevice::from_raw(0x111111)),
        };
        host.upsert_device(device);
        assert_eq!(host.devices.len(), 1); // Should still be only one device
        let device = host.devices.get(&addr).unwrap();
        assert_eq!(device.addr, addr);
        assert_eq!(device.rssi, Some(-40)); // Updated
        assert_eq!(device.name, Some(new_name)); // Updated
        assert_eq!(
            device.class_of_device,
            Some(ClassOfDevice::from_raw(0x111111))
        ); // Unchanged
    }

    #[test]
    fn test_add_or_update_device_partial_update() {
        let mut host = create_test_host();
        let addr = create_test_address([0x01, 0x02, 0x03, 0x04, 0x05, 0x06]);
        let name = heapless::String::try_from("Test").unwrap();
        // Add initial device
        let device = BluetoothDevice {
            addr,
            rssi: Some(-60),
            name: Some(name.clone()),
            class_of_device: Some(ClassOfDevice::from_raw(0x111111)),
        };
        host.upsert_device(device);
        // Update only RSSI
        let device = BluetoothDevice {
            addr,
            rssi: Some(-30),
            name: Some(name.clone()),
            class_of_device: Some(ClassOfDevice::from_raw(0x111111)),
        };
        host.upsert_device(device);
        let device = host.devices.get(&addr).unwrap();
        assert_eq!(device.rssi, Some(-30)); // Updated
        assert_eq!(device.name, Some(name)); // Unchanged
        assert_eq!(
            device.class_of_device,
            Some(ClassOfDevice::from_raw(0x111111))
        ); // Unchanged
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
            let device = BluetoothDevice {
                addr,
                rssi: Some(-(i as i8) - 30),
                name: None,
                class_of_device: None,
            };
            host.upsert_device(device);
        }

        assert_eq!(host.devices.len(), constants::MAX_DISCOVERED_DEVICES);

        // Adding one more should not exceed the limit (heapless behavior)
        let extra_addr = create_test_address([0xFF, 0x02, 0x03, 0x04, 0x05, 0x06]);
        let device = BluetoothDevice {
            addr: extra_addr,
            rssi: Some(-100),
            name: None,
            class_of_device: None,
        };
        host.upsert_device(device);

        // Device count should not exceed max (heapless map handles this)
        assert!(host.devices.len() <= constants::MAX_DISCOVERED_DEVICES);
    }

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
        let device1 = BluetoothDevice {
            addr: addr1,
            rssi: Some(-45),
            name: None,
            class_of_device: Some(ClassOfDevice::from_raw(0x240404)),
        };
        let device2 = BluetoothDevice {
            addr: addr2,
            rssi: Some(-55),
            name: None,
            class_of_device: Some(ClassOfDevice::from_raw(0x1F00)),
        };
        host.upsert_device(device1);
        host.upsert_device(device2);

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
        let device = BluetoothDevice {
            addr,
            rssi: Some(-50),
            name: None,
            class_of_device: None,
        };
        host.upsert_device(device);
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
    fn test_edge_cases() {
        let mut host = create_test_host();

        // Test with null address
        let null_addr = create_test_address([0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
        let device = BluetoothDevice {
            addr: null_addr,
            rssi: None,
            name: None,
            class_of_device: None,
        };
        host.upsert_device(device);
        assert!(host.devices.contains_key(&null_addr));

        // Test with broadcast address
        let broadcast_addr = create_test_address([0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
        let device = BluetoothDevice {
            addr: broadcast_addr,
            rssi: None,
            name: None,
            class_of_device: None,
        };
        host.upsert_device(device);
        assert!(host.devices.contains_key(&broadcast_addr));

        // Test class of device edge cases
        assert_eq!(BluetoothDevice::parse_class_of_device(&[0, 0, 0]), None);
        assert_eq!(
            BluetoothDevice::parse_class_of_device(&[0x12, 0x34, 0x56]),
            Some(ClassOfDevice::from_raw(0x563412))
        );
    }

    #[test]
    fn test_concurrent_operations() {
        let mut host = create_test_host();

        // Simulate concurrent discovery and device updates
        host.state = BluetoothState::Discovering;
        host.discovering = true;

        let addr = create_test_address([0x01, 0x02, 0x03, 0x04, 0x05, 0x06]);

        let device = BluetoothDevice {
            addr,
            rssi: Some(-70),
            name: None,
            class_of_device: Some(ClassOfDevice::from_raw(0x123456)),
        };
        host.upsert_device(device);
        assert!(host.devices.contains_key(&addr));

        // Multiple updates to same device (simulating multiple inquiry results)
        let device = BluetoothDevice {
            addr,
            rssi: Some(-60),
            name: None,
            class_of_device: Some(ClassOfDevice::from_raw(0x123456)),
        };
        host.upsert_device(device);
        let device = BluetoothDevice {
            addr,
            rssi: Some(-55),
            name: None,
            class_of_device: Some(ClassOfDevice::from_raw(0x123456)),
        };
        host.upsert_device(device);
        let name = [
            b'T', b'e', b's', b't', 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0,
        ];
        host.update_device_name(addr, name);

        let device = host.devices.get(&addr).unwrap();
        let expected_name = heapless::String::try_from("Test").unwrap();
        assert_eq!(device.rssi, Some(-55)); // Last RSSI update
        assert_eq!(device.name, Some(expected_name)); // Name from last update
        assert_eq!(
            device.class_of_device,
            Some(ClassOfDevice::from_raw(0x123456))
        ); // CoD from middle update
    }
}
