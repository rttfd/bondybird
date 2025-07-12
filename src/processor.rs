//! # Processor
//!
//! HCI Event, API Request, and Internal Command processing
//!
//! This module contains the main processing tasks that handle HCI events, API requests,
//! and internal commands in parallel. All tasks share the `BluetoothHost` state via a
//! mutex for thread-safe access.
//!
//! # Usage
//!
//! Use the `run` function to start all processor tasks:
//!
//! ```rust,ignore
//! use bondybird::{processor, BluetoothHostOptions};
//! use bt_hci::controller::ExternalController;
//!
//! fn example() {
//! let options = BluetoothHostOptions::default();
//!     let transport = YourTransport::new();
//!     let controller = ExternalController::new(transport);
//!     let controller_ref = Box::leak(Box::new(controller));
//!
//!     // This spawns all three processor tasks internally
//!     processor::run::<YourTransport, 4, 512>(options, controller_ref).await;
//! }
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
//!
//! # Example: Complete Bluetooth Flow
//!
//! ```rust,no_run,ignore
//! use bondybird::{processor, BluetoothHostOptions};
//! use embassy_time::{Duration, Timer};
//!
//! async fn bluetooth_example() -> Result<(), bondybird::BluetoothError> {
//!     let options = BluetoothHostOptions {
//!         lap: [0x33, 0x8B, 0x9E],       // GIAC - General Inquiry Access Code
//!         inquiry_length: 8,              // 8 * 1.28s = ~10 seconds
//!         num_responses: 10,              // Maximum 10 device responses
//!     };
//!     
//!     // Start all processor tasks (normally done in embassy spawner)
//!     // processor::run::<YourTransport, 4, 512>(options, controller_ref).await;
//!     
//!     // Use API functions (these work because processors are running)
//!     bondybird::api::start_discovery().await?;
//!     Timer::after(Duration::from_secs(5)).await;
//!     let devices = bondybird::api::get_devices().await?;
//!     println!("Found {} devices", devices.len());
//!     
//!     if let Some(device) = devices.first() {
//!         let addr_str = device.addr.format_hex();
//!         bondybird::api::connect_device(&addr_str).await?;
//!         println!("Connected to device: {}", addr_str);
//!     }
//!     
//!     let state = bondybird::api::get_state().await?;
//!     println!("Bluetooth state: {:?}", state);
//!     Ok(())
//! }
//! ```
//!
//! # Note
//!
//! Ensure that the HCI transport and controller are correctly set up for your hardware
//! platform. The example above assumes a generic `YourTransport` type; replace this
//! with the actual transport type used in your project.

use crate::{
    BluetoothHost, BluetoothHostOptions, BluetoothState, INTERNAL_COMMAND_CHANNEL, InternalCommand,
    REQUEST_CHANNEL, RESPONSE_CHANNEL, bluetooth_host,
};
use bt_hci::{
    ControllerToHostPacket,
    controller::{Controller, ExternalController},
    event::{self},
    transport::Transport,
};
use heapless::Vec;

async fn hci_event_processor<
    T: Transport + 'static,
    const SLOTS: usize,
    const BUFFER_SIZE: usize,
>(
    controller: &'static ExternalController<T, SLOTS>,
) -> ! {
    let mut read_buffer = [0u8; BUFFER_SIZE];

    loop {
        defmt::debug!("[HCI_EVENT] Waiting for HCI event...");
        match controller.read(&mut read_buffer).await {
            Ok(packet) => match packet {
                ControllerToHostPacket::Event(event) => {
                    defmt::debug!("[HCI_EVENT] HCI event: {:?}", defmt::Debug2Format(&event));
                    let commands = process_hci_event(&event);
                    for command in commands {
                        INTERNAL_COMMAND_CHANNEL.sender().send(command).await;
                    }
                }
                ControllerToHostPacket::Acl(acl) => {
                    defmt::debug!("[HCI_EVENT] HCI ACL: {:?}", defmt::Debug2Format(&acl));
                }
                ControllerToHostPacket::Sync(sync) => {
                    defmt::debug!("[HCI_EVENT] HCI SYNC: {:?}", defmt::Debug2Format(&sync));
                }
                ControllerToHostPacket::Iso(iso) => {
                    defmt::debug!("[HCI_EVENT] HCI ISO: {:?}", defmt::Debug2Format(&iso));
                }
            },
            Err(e) => {
                defmt::error!("[HCI_EVENT] HCI read error: {:?}", defmt::Debug2Format(&e));
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

async fn api_request_processor<T: Transport + 'static, const SLOTS: usize>(
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
                defmt::error!("[API] BluetoothHost not initialized: {}", e);
                // Optionally: panic or return here if host is required
            }
        }
    }

    let api_receiver = REQUEST_CHANNEL.receiver();
    let api_sender = RESPONSE_CHANNEL.sender();

    loop {
        let api_request = api_receiver.receive().await;
        defmt::debug!("[API] API request: {:?}", defmt::Debug2Format(&api_request));
        let response = {
            match bluetooth_host().await {
                Ok(mut host) => host.process_api_request(api_request, controller).await,
                Err(e) => {
                    defmt::error!("[API] BluetoothHost not initialized: {}", e);
                    // Return an error response if host is not initialized
                    crate::Response::Error(crate::BluetoothError::InitializationFailed)
                }
            }
        };
        defmt::debug!("[API] API response: {:?}", defmt::Debug2Format(&response));
        api_sender.send(response).await;
    }
}

async fn internal_command_processor<T: Transport + 'static, const SLOTS: usize>(
    controller: &'static ExternalController<T, SLOTS>,
) -> ! {
    let internal_receiver = INTERNAL_COMMAND_CHANNEL.receiver();

    loop {
        let internal_command = internal_receiver.receive().await;
        defmt::debug!(
            "[COMMAND] Internal command: {:?}",
            defmt::Debug2Format(&internal_command)
        );
        match bluetooth_host().await {
            Ok(mut host) => {
                host.process_internal_command(internal_command, controller)
                    .await;
            }
            Err(e) => defmt::error!("[COMMAND] BluetoothHost not initialized: {}", e),
        }
        // No response needed for internal commands
    }
}

/// Run the Bluetooth host processor tasks
///
/// # Panics
///
/// This function will panic if Bluetooth host initialization fails.
/// The panic occurs if `init_bluetooth_host(options)` returns an error.
///
pub async fn run<T: Transport + 'static, const SLOTS: usize, const BUFFER_SIZE: usize>(
    options: BluetoothHostOptions,
    controller: &'static ExternalController<T, SLOTS>,
) {
    crate::init_bluetooth_host(options)
        .await
        .expect("Failed to initialize Bluetooth host");

    embassy_futures::select::select3(
        hci_event_processor::<T, SLOTS, BUFFER_SIZE>(controller),
        api_request_processor::<T, SLOTS>(controller),
        internal_command_processor::<T, SLOTS>(controller),
    )
    .await;
}
