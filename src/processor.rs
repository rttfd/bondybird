//! Processor Tasks - HCI Event and API Request processing
//!
//! This module contains the main processing tasks that handle HCI events and API requests
//! in parallel. Both tasks share the `BluetoothHost` state via a mutex for thread-safe access.

use crate::{BLUETOOTH_HOST, BluetoothState, REQUEST_CHANNEL, RESPONSE_CHANNEL};
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
                    BLUETOOTH_HOST.lock().await.process_hci_event(&event);
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
        let mut data = BLUETOOTH_HOST.lock().await;
        if data.initialize(controller).await.is_err() {
            data.state = BluetoothState::PoweredOff;
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
            let mut data = BLUETOOTH_HOST.lock().await;
            data.process_api_request(api_request, controller).await
        };
        defmt::debug!(
            "[PROCESSOR] API response: {:?}",
            defmt::Debug2Format(&response)
        );
        api_sender.send(response).await;
    }
}
