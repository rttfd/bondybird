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
    BluetoothDevice, BluetoothError, BluetoothState, REQUEST_CHANNEL, RESPONSE_CHANNEL, Request,
    Response, constants::MAX_DISCOVERED_DEVICES,
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
        Response::DiscoverComplete => Ok(()),
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
