//! Processor Tasks - HCI Event and API Request processing
//!
//! This module contains the main processing tasks that handle HCI events and API requests
//! in parallel. Both tasks share the `BluetoothHost` state via a mutex for thread-safe access.

use crate::{BluetoothState, REQUEST_CHANNEL, RESPONSE_CHANNEL, bluetooth_host};
use bt_hci::{
    ControllerToHostPacket,
    controller::{Controller, ExternalController},
    transport::Transport,
};

/// HCI Event Processor Task
pub async fn hci_event_processor<T: Transport + 'static, const SLOTS: usize>(
    controller: &'static ExternalController<T, SLOTS>,
) -> ! {
    let mut read_buffer = [0u8; 256];

    loop {
        defmt::debug!("[PROCESSOR] Waiting for HCI event...");
        match controller.read(&mut read_buffer).await {
            Ok(packet) => match packet {
                ControllerToHostPacket::Event(event) => {
                    defmt::debug!("[PROCESSOR] HCI event: {:?}", defmt::Debug2Format(&event));
                    match bluetooth_host().await {
                        Ok(mut host) => host.process_hci_event(&event),
                        Err(e) => defmt::error!("[PROCESSOR] BluetoothHost not initialized: {}", e),
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

/// API Request Processor Task
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
