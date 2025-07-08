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
