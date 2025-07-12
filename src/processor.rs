//! Processor Tasks - HCI Event, API Request, and Internal Command processing
//!
//! This module contains the main processing tasks that handle HCI events, API requests,
//! and internal commands in parallel. All tasks share the `BluetoothHost` state via a
//! mutex for thread-safe access.
//!
//! # Usage
//!
//! These processor tasks should be spawned as separate Embassy tasks:
//!
//! ```ignore
//! use bondybird::processor::{hci_event_processor, api_request_processor, internal_command_processor};
//!
//! // In your Embassy spawner
//! spawner.spawn(hci_event_processor::<Transport, 4, 512>(controller)).unwrap();
//! spawner.spawn(api_request_processor::<Transport, 4>(controller)).unwrap();
//! spawner.spawn(internal_command_processor::<Transport, 4>(controller)).unwrap();
//! ```
//!
//! # Architecture
//!
//! * **HCI Event Processor**: Handles incoming HCI events and sends internal commands
//! * **API Request Processor**: Handles external API requests with responses  
//! * **Internal Command Processor**: Executes HCI commands triggered by events (no responses)
//!
//! # Generic Parameters
//!
//! * `T: Transport` - The HCI transport layer (UART, USB, etc.)
//! * `SLOTS` - Maximum number of controller command slots (typically 4-8)
//! * `BUFFER_SIZE` - Size of HCI read buffer in bytes (512+ recommended)

use crate::{
    BluetoothHost, BluetoothState, INTERNAL_COMMAND_CHANNEL, InternalCommand, REQUEST_CHANNEL,
    RESPONSE_CHANNEL, bluetooth_host,
};
use bt_hci::{
    ControllerToHostPacket,
    controller::{Controller, ExternalController},
    event::{self},
    transport::Transport,
};
use heapless::Vec;

/// HCI Event Processor Task
///
/// Processes incoming HCI events from the Bluetooth controller in a continuous loop.
/// This task handles all types of HCI packets including events, ACL data, synchronous data, and ISO data.
///
/// # Generic Parameters
///
/// * `T` - Transport layer implementation for HCI communication
/// * `SLOTS` - Maximum number of controller slots for command queuing
/// * `BUFFER_SIZE` - Size of the read buffer for incoming HCI packets (typically 512 or larger)
///
/// # Arguments
///
/// * `controller` - Reference to the external Bluetooth controller
///
/// # Behavior
///
/// This function runs indefinitely, continuously reading HCI packets from the controller
/// and dispatching them to the appropriate handlers in the `BluetoothHost`.
pub async fn hci_event_processor<
    T: Transport + 'static,
    const SLOTS: usize,
    const BUFFER_SIZE: usize,
>(
    controller: &'static ExternalController<T, SLOTS>,
) -> ! {
    let mut read_buffer = [0u8; BUFFER_SIZE];

    loop {
        defmt::debug!("[PROCESSOR] Waiting for HCI event...");
        match controller.read(&mut read_buffer).await {
            Ok(packet) => match packet {
                ControllerToHostPacket::Event(event) => {
                    defmt::debug!("[PROCESSOR] HCI event: {:?}", defmt::Debug2Format(&event));
                    let commands = process_hci_event(&event);
                    for command in commands {
                        INTERNAL_COMMAND_CHANNEL.sender().send(command).await;
                    }
                }
                ControllerToHostPacket::Acl(acl) => {
                    defmt::debug!("[PROCESSOR] HCI ACL: {:?}", defmt::Debug2Format(&acl));
                }
                ControllerToHostPacket::Sync(sync) => {
                    defmt::debug!("[PROCESSOR] HCI SYNC: {:?}", defmt::Debug2Format(&sync));
                }
                ControllerToHostPacket::Iso(iso) => {
                    defmt::debug!("[PROCESSOR] HCI ISO: {:?}", defmt::Debug2Format(&iso));
                }
            },
            Err(e) => {
                defmt::error!("[PROCESSOR] HCI read error: {:?}", defmt::Debug2Format(&e));
            }
        }
    }
}

/// Process HCI events and return InternalCommand(s) for all mutations
fn process_hci_event(event: &event::Event<'_>) -> Vec<InternalCommand, 8> {
    let mut commands = Vec::new();
    match *event {
        event::Event::InquiryResult(ref result) => {
            for res in result.iter() {
                if let Ok(device) = res.try_into() {
                    commands.push(InternalCommand::UpsertDevice(device)).ok();
                }
            }
        }
        event::Event::InquiryComplete(ref complete) => {
            commands.push(InternalCommand::SetDiscovering(false)).ok();
            if complete.status.to_result().is_ok() {
                commands
                    .push(InternalCommand::SetState(BluetoothState::PoweredOn))
                    .ok();
            }
        }
        event::Event::ConnectionComplete(ref complete) => {
            if complete.status.to_result().is_ok() {
                if let Ok(addr) = complete.bd_addr.try_into() {
                    let conn_handle = complete.handle.raw();
                    commands
                        .push(InternalCommand::AddConnection(addr, conn_handle))
                        .ok();
                    commands
                        .push(InternalCommand::SetState(BluetoothState::Connected))
                        .ok();
                    commands
                        .push(InternalCommand::AuthenticationRequested { conn_handle })
                        .ok();
                }
            } else {
                commands
                    .push(InternalCommand::SetState(BluetoothState::PoweredOn))
                    .ok();
            }
        }
        event::Event::DisconnectionComplete(ref complete) => {
            if complete.status.to_result().is_ok() {
                let conn_handle = complete.handle.raw();
                commands
                    .push(InternalCommand::RemoveConnection(conn_handle))
                    .ok();
                commands
                    .push(InternalCommand::SetState(BluetoothState::PoweredOn))
                    .ok();
            }
        }
        event::Event::RemoteNameRequestComplete(ref complete) => {
            if complete.status.to_result().is_ok() {
                if let Ok(addr) = complete.bd_addr.try_into() {
                    let name = BluetoothHost::copy_device_name(&complete.remote_name);
                    commands
                        .push(InternalCommand::UpdateDeviceName(addr, name))
                        .ok();
                }
            }
        }
        event::Event::ExtendedInquiryResult(ref result) => {
            if let Ok(device) = result.try_into() {
                commands.push(InternalCommand::UpsertDevice(device)).ok();
            }
        }
        event::Event::InquiryResultWithRssi(ref result) => {
            for res in result.iter() {
                if let Ok(device) = res.try_into() {
                    commands.push(InternalCommand::UpsertDevice(device)).ok();
                }
            }
        }
        event::Event::PinCodeRequest(ref request) => {
            if let Ok(addr) = request.bd_addr.try_into() {
                let mut pin_code = [0u8; 16];
                pin_code[0] = b'0';
                pin_code[1] = b'0';
                pin_code[2] = b'0';
                pin_code[3] = b'0';
                commands
                    .push(InternalCommand::PinCodeRequestReply {
                        bd_addr: addr,
                        pin_code,
                    })
                    .ok();
            }
        }
        event::Event::LinkKeyRequest(ref request) => {
            if let Ok(addr) = request.bd_addr.try_into() {
                commands
                    .push(InternalCommand::LinkKeyRequestNegativeReply { bd_addr: addr })
                    .ok();
            }
        }
        event::Event::IoCapabilityRequest(ref request) => {
            if let Ok(addr) = request.bd_addr.try_into() {
                commands
                    .push(InternalCommand::IoCapabilityRequestReply { bd_addr: addr })
                    .ok();
            }
        }
        event::Event::UserConfirmationRequest(ref request) => {
            if let Ok(addr) = request.bd_addr.try_into() {
                commands
                    .push(InternalCommand::UserConfirmationRequestReply { bd_addr: addr })
                    .ok();
            }
        }
        event::Event::LinkKeyNotification(ref notification) => {
            if let Ok(addr) = notification.bd_addr.try_into() {
                commands
                    .push(InternalCommand::StoreLinkKey(addr, notification.link_key))
                    .ok();
            }
        }
        event::Event::AuthenticationComplete(ref complete) => {
            if complete.status.to_result().is_ok() {
                // On successful authentication, set state to PoweredOn
                commands
                    .push(InternalCommand::SetState(BluetoothState::PoweredOn))
                    .ok();
                // Optionally remove connection if needed (depends on your protocol)
                // If you want to remove connection on auth complete, uncomment below:
                // let conn_handle = complete.handle.raw();
                // commands.push(InternalCommand::RemoveConnection(conn_handle)).ok();
            } else {
                // On failed authentication, set state to PoweredOn (or error state)
                commands
                    .push(InternalCommand::SetState(BluetoothState::PoweredOn))
                    .ok();
            }
        }
        _ => {
            // Handle other events if necessary, currently ignored
            defmt::debug!("[EVENT] Unhandled event: {:?}", event);
        }
    }
    commands
}

/// API Request Processor Task
///
/// Processes API requests from the application and coordinates with the Bluetooth controller.
/// This task initializes the `BluetoothHost` on startup and then continuously processes
/// incoming API requests, sending responses back through the response channel.
///
/// # Generic Parameters
///
/// * `T` - Transport layer implementation for HCI communication
/// * `SLOTS` - Maximum number of controller slots for command queuing
///
/// # Arguments
///
/// * `controller` - Reference to the external Bluetooth controller
///
/// # Behavior
///
/// 1. Initializes the `BluetoothHost` with the provided controller
/// 2. Runs indefinitely, processing API requests and sending responses
/// 3. Handles `BluetoothHost` initialization errors gracefully
pub async fn api_request_processor<T: Transport + 'static, const SLOTS: usize>(
    controller: &'static ExternalController<T, SLOTS>,
) -> ! {
    {
        match bluetooth_host().await {
            Ok(mut host) => {
                if host.initialize(controller).await.is_err() {
                    host.state = BluetoothState::PoweredOff;
                }
            }
            Err(e) => {
                defmt::error!("[PROCESSOR] BluetoothHost not initialized: {}", e);
                // Optionally: panic or return here if host is required
            }
        }
    }

    let api_receiver = REQUEST_CHANNEL.receiver();
    let api_sender = RESPONSE_CHANNEL.sender();

    loop {
        let api_request = api_receiver.receive().await;
        defmt::debug!(
            "[PROCESSOR] API request: {:?}",
            defmt::Debug2Format(&api_request)
        );
        let response = {
            match bluetooth_host().await {
                Ok(mut host) => host.process_api_request(api_request, controller).await,
                Err(e) => {
                    defmt::error!("[PROCESSOR] BluetoothHost not initialized: {}", e);
                    // Return an error response if host is not initialized
                    crate::Response::Error(crate::BluetoothError::InitializationFailed)
                }
            }
        };
        defmt::debug!(
            "[PROCESSOR] API response: {:?}",
            defmt::Debug2Format(&response)
        );
        api_sender.send(response).await;
    }
}

/// Internal Command Processor Task
///
/// Processes internal commands from HCI event processing (fire-and-forget, no responses).
/// This task handles internal HCI commands that are triggered by events and don't need
/// to send responses back to any API callers.
///
/// # Generic Parameters
///
/// * `T` - Transport layer implementation for HCI communication
/// * `SLOTS` - Maximum number of controller slots for command queuing
///
/// # Arguments
///
/// * `controller` - Reference to the external Bluetooth controller
///
/// # Behavior
///
/// Runs indefinitely, processing internal commands as they arrive from the event processor.
/// These commands execute HCI operations without sending responses.
pub async fn internal_command_processor<T: Transport + 'static, const SLOTS: usize>(
    controller: &'static ExternalController<T, SLOTS>,
) -> ! {
    let internal_receiver = INTERNAL_COMMAND_CHANNEL.receiver();

    loop {
        let internal_command = internal_receiver.receive().await;
        defmt::debug!(
            "[PROCESSOR] Internal command: {:?}",
            defmt::Debug2Format(&internal_command)
        );
        match bluetooth_host().await {
            Ok(mut host) => {
                host.process_internal_command(internal_command, controller)
                    .await;
            }
            Err(e) => defmt::error!("[PROCESSOR] BluetoothHost not initialized: {}", e),
        }
        // No response needed for internal commands
    }
}
