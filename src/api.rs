//! BondyBird API Functions
//!
//! This module provides the public API functions for interacting with the Bluetooth manager.
//! These functions use static channels to communicate with the manager task and are designed
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
//!
//! // Start device discovery
//! let _ = start_discovery().await;
//!
//! // Get discovered devices
//! let devices = get_devices().await.unwrap();
//!
//! // Connect to a device
//! if let Some(device) = devices.first() {
//!     let addr_str = format_address(device.addr);
//!     let _ = connect_device(addr_str.into()).await;
//! }
//! ```

use crate::{
    API_REQUEST_CHANNEL, API_RESPONSE_CHANNEL, ApiRequest, ApiResponse, BluetoothDevice,
    BluetoothError, BluetoothState, MAX_DISCOVERED_DEVICES,
};
use heapless::{String, Vec};

/// Start device discovery
///
/// This function starts the device discovery process by sending a request to the Bluetooth manager.
/// It will cause the manager to begin scanning for nearby Bluetooth devices.
///
/// # Errors
///
/// Returns a `BluetoothError` if:
/// - The discovery is already in progress (`AlreadyInProgress`)
/// - The HCI command fails (`HciError`)
/// - The response is unexpected (returns `HciError`)
pub async fn start_discovery() -> Result<(), BluetoothError> {
    let sender = API_REQUEST_CHANNEL.sender();
    let receiver = API_RESPONSE_CHANNEL.receiver();

    sender.send(ApiRequest::Discover).await;

    match receiver.receive().await {
        ApiResponse::DiscoverComplete => Ok(()),
        ApiResponse::Error(e) => Err(e),
        _ => Err(BluetoothError::HciError),
    }
}

/// Get list of discovered devices
///
/// This function retrieves the list of devices that have been discovered during
/// the most recent discovery operation.
///
/// # Errors
///
/// Returns `BluetoothError::HciError` if there is an issue communicating with the Bluetooth controller,
/// or other specific `BluetoothError` variants as returned by the manager.
pub async fn get_devices() -> Result<Vec<BluetoothDevice, MAX_DISCOVERED_DEVICES>, BluetoothError> {
    let sender = API_REQUEST_CHANNEL.sender();
    let receiver = API_RESPONSE_CHANNEL.receiver();

    sender.send(ApiRequest::GetDevices).await;

    match receiver.receive().await {
        ApiResponse::Devices(devices) => Ok(devices),
        ApiResponse::Error(e) => Err(e),
        _ => Err(BluetoothError::HciError),
    }
}

/// Connect to a device
///
/// This function attempts to establish a connection with a Bluetooth device at the specified address.
/// The device must have been previously discovered before attempting to connect.
///
/// # Arguments
///
/// * `address` - The Bluetooth address of the device to connect to (e.g., "00:11:22:33:44:55")
///
/// # Errors
///
/// Returns a `BluetoothError` if:
/// - The address format is invalid (`InvalidParameter`)
/// - The device was not previously discovered (`DeviceNotFound`)
/// - The connection attempt fails (`ConnectionFailed`)
/// - The response is unexpected (returns `HciError`)
pub async fn connect_device(address: String<64>) -> Result<(), BluetoothError> {
    let sender = API_REQUEST_CHANNEL.sender();
    let receiver = API_RESPONSE_CHANNEL.receiver();

    sender.send(ApiRequest::Pair(address)).await;

    match receiver.receive().await {
        ApiResponse::PairComplete => Ok(()),
        ApiResponse::Error(e) => Err(e),
        _ => Err(BluetoothError::HciError),
    }
}

/// Disconnect from a device
///
/// This function disconnects from a currently connected Bluetooth device.
///
/// # Arguments
///
/// * `address` - The Bluetooth address of the device to disconnect from (e.g., "00:11:22:33:44:55")
///
/// # Errors
///
/// Returns a `BluetoothError` if:
/// - The address format is invalid (`InvalidParameter`)
/// - The device is not currently connected (`DeviceNotFound`)
/// - The HCI command fails (`HciError`)
/// - The response is unexpected (returns `HciError`)
pub async fn disconnect_device(address: String<64>) -> Result<(), BluetoothError> {
    let sender = API_REQUEST_CHANNEL.sender();
    let receiver = API_RESPONSE_CHANNEL.receiver();

    sender.send(ApiRequest::Disconnect(address)).await;

    match receiver.receive().await {
        ApiResponse::DisconnectComplete => Ok(()),
        ApiResponse::Error(e) => Err(e),
        _ => Err(BluetoothError::HciError),
    }
}

/// Get current Bluetooth state
///
/// This function retrieves the current state of the Bluetooth system,
/// such as whether it's powered on, discovering, or connected.
///
/// # Errors
///
/// Returns a `BluetoothError` if:
/// - The HCI command fails (`HciError`)
/// - The response is unexpected (returns `HciError`)
pub async fn get_state() -> Result<BluetoothState, BluetoothError> {
    let sender = API_REQUEST_CHANNEL.sender();
    let receiver = API_RESPONSE_CHANNEL.receiver();

    sender.send(ApiRequest::GetState).await;

    match receiver.receive().await {
        ApiResponse::State(state) => Ok(state),
        ApiResponse::Error(e) => Err(e),
        _ => Err(BluetoothError::HciError),
    }
}
