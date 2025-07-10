//! `BondyBird` API Functions
//!
//! This module provides the public API functions for interacting with the Bluetooth processing tasks.
//! These functions use static channels to communicate with the processing tasks and are designed
//! to be called from application code.
//!
//! The API functions are generic and not coupled to any specific transport or protocol.
//! They can be used in web servers, REST APIs, CLI applications, or any other application
//! architecture.
//!
//! # Usage
//!
//! ```rust,no_run
//! use bondybird::api::{start_discovery, get_devices, connect_device};
//! # async fn example() {
//!
//! // Start device discovery
//! let _ = start_discovery().await;
//!
//! // Get discovered devices
//! let devices = get_devices().await.unwrap();
//!
//! // Connect to a device
//! if let Some(device) = devices.first() {
//!     let addr_str = device.addr.format_hex();
//!     let addr_64: heapless::String<64> = heapless::String::try_from(addr_str.as_str()).unwrap();
//!     let _ = connect_device(addr_64).await;
//! }
//! # }
//! ```

use crate::{
    BluetoothDevice, BluetoothError, BluetoothState, LocalDeviceInfo, REQUEST_CHANNEL,
    RESPONSE_CHANNEL, Request, Response, constants::MAX_DISCOVERED_DEVICES,
};
use heapless::{String, Vec};

/// Start Bluetooth device discovery.
///
/// # Errors
///
/// Returns an error if discovery is already in progress, the HCI command fails, or the response is unexpected.
pub async fn start_discovery() -> Result<(), BluetoothError> {
    REQUEST_CHANNEL.sender().send(Request::Discover).await;
    match RESPONSE_CHANNEL.receiver().receive().await {
        Response::DiscoverStarted => Ok(()),
        Response::Error(e) => Err(e),
        _ => Err(BluetoothError::HciError),
    }
}

/// Get the list of discovered Bluetooth devices.
///
/// # Errors
///
/// Returns an error if communication fails or the response is unexpected.
pub async fn get_devices() -> Result<Vec<BluetoothDevice, MAX_DISCOVERED_DEVICES>, BluetoothError> {
    REQUEST_CHANNEL.sender().send(Request::GetDevices).await;
    match RESPONSE_CHANNEL.receiver().receive().await {
        Response::Devices(devices) => Ok(devices),
        Response::Error(e) => Err(e),
        _ => Err(BluetoothError::HciError),
    }
}

/// Connect to a Bluetooth device by address (must be previously discovered).
///
/// # Errors
///
/// Returns an error if the address is invalid, the device is not found, connection fails, or the response is unexpected.
pub async fn connect_device(address: String<64>) -> Result<(), BluetoothError> {
    REQUEST_CHANNEL.sender().send(Request::Pair(address)).await;
    match RESPONSE_CHANNEL.receiver().receive().await {
        Response::PairComplete => Ok(()),
        Response::Error(e) => Err(e),
        _ => Err(BluetoothError::HciError),
    }
}

/// Disconnect from a Bluetooth device by address.
///
/// # Errors
///
/// Returns an error if the address is invalid, the device is not connected, the command fails, or the response is unexpected.
pub async fn disconnect_device(address: String<64>) -> Result<(), BluetoothError> {
    REQUEST_CHANNEL
        .sender()
        .send(Request::Disconnect(address))
        .await;
    match RESPONSE_CHANNEL.receiver().receive().await {
        Response::DisconnectComplete => Ok(()),
        Response::Error(e) => Err(e),
        _ => Err(BluetoothError::HciError),
    }
}

/// Get the current Bluetooth state (powered on, discovering, connected, etc).
///
/// # Errors
///
/// Returns an error if the command fails or the response is unexpected.
pub async fn get_state() -> Result<BluetoothState, BluetoothError> {
    REQUEST_CHANNEL.sender().send(Request::GetState).await;
    match RESPONSE_CHANNEL.receiver().receive().await {
        Response::State(state) => Ok(state),
        Response::Error(e) => Err(e),
        _ => Err(BluetoothError::HciError),
    }
}

/// Get local Bluetooth information (e.g., device name, address).
///
/// # Errors
///
/// Returns an error if the command fails or the response is unexpected.
pub async fn get_local_info() -> Result<LocalDeviceInfo, BluetoothError> {
    REQUEST_CHANNEL.sender().send(Request::GetLocalInfo).await;
    match RESPONSE_CHANNEL.receiver().receive().await {
        Response::LocalInfo(info) => Ok(info),
        Response::Error(e) => Err(e),
        _ => Err(BluetoothError::HciError),
    }
}

/// Get the list of paired/connected Bluetooth devices.
///
/// # Errors
///
/// Returns an error if communication fails or the response is unexpected.
pub async fn get_paired_devices()
-> Result<Vec<BluetoothDevice, MAX_DISCOVERED_DEVICES>, BluetoothError> {
    REQUEST_CHANNEL
        .sender()
        .send(Request::GetPairedDevices)
        .await;
    match RESPONSE_CHANNEL.receiver().receive().await {
        Response::PairedDevices(devices) => Ok(devices),
        Response::Error(e) => Err(e),
        _ => Err(BluetoothError::HciError),
    }
}

/// Get the name of a specific Bluetooth device by address.
///
/// This function performs a Remote Name Request to retrieve the human-readable name
/// of a Bluetooth device. The device must be discoverable and within range.
///
/// # Arguments
///
/// * `address` - The Bluetooth device address as a string (e.g., "12:34:56:78:9A:BC")
///
/// # Returns
///
/// * `Ok((address, name))` - The device address and name as a heapless string
/// * `Err(BluetoothError)` - If the request fails or the device is not found
///
/// # Errors
///
/// Returns an error if:
/// - The address format is invalid
/// - The device is not reachable
/// - The Remote Name Request times out
/// - The response is unexpected
///
/// # Example
///
/// ```rust,no_run
/// use bondybird::api::get_device_name;
/// use heapless::String;
///
/// # async fn example() -> Result<(), bondybird::BluetoothError> {
/// let address: String<64> = String::try_from("12:34:56:78:9A:BC").unwrap();
/// let (addr, name) = get_device_name(address).await?;
///
/// println!("Device {} is named: {}", addr.as_str(), name.as_str());
/// # Ok(())
/// # }
/// ```
pub async fn get_device_name(
    address: String<64>,
) -> Result<(String<64>, String<32>), BluetoothError> {
    REQUEST_CHANNEL
        .sender()
        .send(Request::GetDeviceName(address))
        .await;
    match RESPONSE_CHANNEL.receiver().receive().await {
        Response::DeviceName(addr, name) => Ok((addr, name)),
        Response::Error(e) => Err(e),
        _ => Err(BluetoothError::HciError),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BluetoothAddress, BluetoothState, ClassOfDevice, LocalDeviceInfo};

    /// Helper function to create a test device
    fn create_test_device() -> BluetoothDevice {
        BluetoothDevice::new(BluetoothAddress::new([0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC]))
            .with_rssi(-45)
            .with_class_of_device(ClassOfDevice::from_raw(0x0024_0404)) // Audio device
    }

    /// Helper function to create test local info
    fn create_test_local_info() -> LocalDeviceInfo {
        LocalDeviceInfo {
            bd_addr: Some(BluetoothAddress::new([0x00, 0x11, 0x22, 0x33, 0x44, 0x55])),
            hci_version: Some(0x0C), // Bluetooth 5.2
            hci_revision: Some(0x1234),
            lmp_version: Some(0x0C),
            manufacturer_name: Some(0x000F), // Broadcom
            lmp_subversion: Some(0x5678),
            local_features: Some([0xFF, 0xFE, 0xCD, 0xFE, 0xDB, 0xFF, 0x7B, 0x87]),
            acl_data_packet_length: Some(1021),
            sco_data_packet_length: Some(64),
            total_num_acl_data_packets: Some(8),
            total_num_sco_data_packets: Some(8),
        }
    }

    #[test]
    fn test_request_enum_variants() {
        // Test that all Request variants can be created
        let requests = [
            Request::Discover,
            Request::StopDiscovery,
            Request::GetDevices,
            Request::Pair(heapless::String::try_from("12:34:56:78:9A:BC").unwrap()),
            Request::Disconnect(heapless::String::try_from("12:34:56:78:9A:BC").unwrap()),
            Request::GetState,
            Request::GetLocalInfo,
            Request::GetPairedDevices,
            Request::GetDeviceName(heapless::String::try_from("12:34:56:78:9A:BC").unwrap()),
        ];

        // Just ensure they can be created and cloned
        for request in requests {
            let _cloned = request.clone();
        }
    }

    #[test]
    fn test_response_enum_variants() {
        let test_device = create_test_device();
        let mut devices = heapless::Vec::new();
        devices.push(test_device.clone()).unwrap();

        let mut paired_devices = heapless::Vec::new();
        paired_devices.push(test_device).unwrap();

        let test_name = heapless::String::try_from("Test Device").unwrap();

        // Test that all Response variants can be created
        let responses = [
            Response::DiscoverComplete,
            Response::DiscoveryStopped,
            Response::Devices(devices),
            Response::PairedDevices(paired_devices),
            Response::PairComplete,
            Response::DisconnectComplete,
            Response::State(BluetoothState::PoweredOn),
            Response::LocalInfo(create_test_local_info()),
            Response::DeviceName(
                heapless::String::try_from("12:34:56:78:9A:BC").unwrap(),
                test_name,
            ),
            Response::Error(BluetoothError::DeviceNotFound),
        ];

        // Just ensure they can be created and cloned
        for response in responses {
            let _cloned = response.clone();
        }
    }

    #[test]
    fn test_get_device_name_api() {
        // Test that the get_device_name function signature is correct
        // This is a compilation test since we don't have a real runtime here

        // Verify that the Request enum includes GetDeviceName
        let address = heapless::String::try_from("12:34:56:78:9A:BC").unwrap();
        let request = Request::GetDeviceName(address.clone());
        match request {
            Request::GetDeviceName(addr) => {
                assert_eq!(addr.as_str(), "12:34:56:78:9A:BC");
            }
            _ => panic!("GetDeviceName variant should exist"),
        }

        // Verify that Response enum includes DeviceName
        let test_name = heapless::String::try_from("Test").unwrap();

        let response = Response::DeviceName(address, test_name.clone());
        match response {
            Response::DeviceName(addr, name) => {
                assert_eq!(addr.as_str(), "12:34:56:78:9A:BC");
                assert_eq!(name.as_str(), "Test");
            }
            _ => panic!("DeviceName variant should exist"),
        }
    }

    #[test]
    fn test_device_name_encoding() {
        // Test that device names are properly handled as heapless strings
        let test_names = [
            "My Headset",
            "",                                // Empty name
            "äöü_device",                      // International characters
            "1234567890123456789012345678901", // Near max length (31 chars)
        ];

        for name_str in test_names {
            if let Ok(name) = heapless::String::<32>::try_from(name_str) {
                let addr_str = heapless::String::try_from("AA:BB:CC:DD:EE:FF").unwrap();
                let response = Response::DeviceName(addr_str.clone(), name.clone());

                match response {
                    Response::DeviceName(addr, received_name) => {
                        assert_eq!(addr.as_str(), "AA:BB:CC:DD:EE:FF");
                        assert_eq!(received_name.as_str(), name_str);
                    }
                    _ => panic!("Should be DeviceName response"),
                }
            }
        }
    }

    #[test]
    fn test_max_discovered_devices_constant() {
        // Test that we can create a Vec with the maximum capacity
        let mut devices: heapless::Vec<BluetoothDevice, MAX_DISCOVERED_DEVICES> =
            heapless::Vec::new();

        // Fill it up to capacity
        for i in 0..MAX_DISCOVERED_DEVICES {
            #[allow(clippy::cast_possible_truncation)]
            let addr = BluetoothAddress::new([(i >> 8) as u8, i as u8, 0x00, 0x00, 0x00, 0x00]);
            let device = BluetoothDevice::new(addr);
            assert!(devices.push(device).is_ok());
        }

        assert_eq!(devices.len(), MAX_DISCOVERED_DEVICES);

        // Try to add one more (should fail)
        let extra_device =
            BluetoothDevice::new(BluetoothAddress::new([0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]));
        assert!(devices.push(extra_device).is_err());
    }

    #[test]
    fn test_device_address_formatting_integration() {
        // Test the integration between BluetoothAddress and the API address string requirements
        let addr = BluetoothAddress::new([0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC]);
        let formatted = addr.format_hex();

        // Should be able to convert the formatted address to the API string type
        let api_address: heapless::String<64> =
            heapless::String::try_from(formatted.as_str()).unwrap();
        assert_eq!(api_address.as_str(), "12:34:56:78:9A:BC");
    }

    #[test]
    fn test_heapless_string_conversion() {
        // Test string conversion for addresses (API requirement)
        let addr_str = "12:34:56:78:9A:BC";
        let heapless_str: heapless::String<64> = heapless::String::try_from(addr_str).unwrap();
        assert_eq!(heapless_str.as_str(), addr_str);
        assert_eq!(heapless_str.len(), 17);
    }

    #[test]
    fn test_heapless_string_max_capacity() {
        // Test that we can store a 64-character string (API requirement)
        let long_str = "1234567890123456789012345678901234567890123456789012345678901234";
        assert_eq!(long_str.len(), 64);

        let heapless_str: heapless::String<64> = heapless::String::try_from(long_str).unwrap();
        assert_eq!(heapless_str.as_str(), long_str);
        assert_eq!(heapless_str.len(), 64);
    }

    #[test]
    fn test_heapless_string_overflow() {
        // Test that strings longer than capacity fail (API boundary test)
        let too_long_str = "12345678901234567890123456789012345678901234567890123456789012345";
        assert_eq!(too_long_str.len(), 65);

        let result: Result<heapless::String<64>, _> = heapless::String::try_from(too_long_str);
        assert!(result.is_err());
    }
}
