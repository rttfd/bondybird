// =============================================================================
// BLUETOOTH MANAGER (HCI EVENT PROCESSOR)
// =============================================================================

use bt_hci::param::{ConnHandle, Status};
use embassy_time::{Duration, Timer};
use heapless::{FnvIndexMap, Vec};

use crate::{
    BluetoothCommand, BluetoothDevice, BluetoothError, BluetoothEvent, BluetoothState,
    CommandReceiver, EVENT_BUFFER_SIZE, EventSender, MAX_CONNECTIONS, MAX_DISCOVERED_DEVICES,
};

/// Bluetooth Manager - Processes HCI events and sends high-level events
pub struct BluetoothManager<T>
where
    T: bt_hci::controller::Controller,
{
    // Core components
    controller: T,
    state: BluetoothState,

    // Device tracking
    discovered_devices: Vec<BluetoothDevice, MAX_DISCOVERED_DEVICES>,
    connections: FnvIndexMap<u16, [u8; 6], MAX_CONNECTIONS>, // handle -> addr mapping

    // Communication channels
    event_sender: EventSender,
    command_receiver: CommandReceiver,

    // State tracking
    discovery_active: bool,
}

impl<T> BluetoothManager<T>
where
    T: bt_hci::controller::Controller
        + bt_hci::controller::ControllerCmdSync<bt_hci::cmd::controller_baseband::Reset>
        + bt_hci::controller::ControllerCmdSync<bt_hci::cmd::controller_baseband::SetEventMask>
        + bt_hci::controller::ControllerCmdSync<bt_hci::cmd::link_control::Inquiry>
        + bt_hci::controller::ControllerCmdSync<bt_hci::cmd::link_control::CreateConnection>
        + bt_hci::controller::ControllerCmdSync<bt_hci::cmd::link_control::Disconnect>,
{
    /// Create a new Bluetooth Manager
    pub fn new(
        controller: T,
        event_sender: EventSender,
        command_receiver: CommandReceiver,
    ) -> Self {
        Self {
            controller,
            state: BluetoothState::PoweredOff,
            discovered_devices: Vec::new(),
            connections: FnvIndexMap::new(),
            event_sender,
            command_receiver,
            discovery_active: false,
        }
    }

    /// Main manager loop - processes HCI events and commands
    pub async fn run(&mut self) {
        defmt::info!("BluetoothManager starting");

        // Signal that manager is ready
        let () = self.event_sender.send(BluetoothEvent::ManagerReady).await;

        loop {
            // First process any pending commands
            self.process_commands().await;

            // Then process HCI events
            self.process_hci_events().await;
        }
    }

    /// Process incoming HCI events from controller
    async fn process_hci_events(&mut self) {
        let mut event_buffer = [0u8; EVENT_BUFFER_SIZE];
        if let Ok(bt_hci::ControllerToHostPacket::Event(event)) =
            self.controller.read(&mut event_buffer).await
        {
            self.handle_hci_event(&event).await;
        }
        Timer::after(Duration::from_millis(1)).await;
    }

    /// Process commands from `EventHandler`
    async fn process_commands(&mut self) {
        if let Ok(command) = self.command_receiver.try_receive() {
            if let Err(e) = self.handle_command(command).await {
                let () = self.event_sender.send(BluetoothEvent::Error(e)).await;
            }
        }
    }

    /// Handle HCI events and convert to high-level events
    async fn handle_hci_event(&mut self, event: &bt_hci::event::Event<'_>) {
        use bt_hci::event::Event;

        match event {
            Event::InquiryResult(_result) => {
                // TODO: Implement inquiry result parsing
                defmt::debug!("Inquiry result received");
            }

            Event::InquiryResultWithRssi(_result) => {
                // TODO: Implement inquiry result with RSSI parsing
                defmt::debug!("Inquiry result with RSSI received");
            }

            Event::InquiryComplete(_) => {
                self.discovery_active = false;
                self.set_state(BluetoothState::Idle).await;
                let () = self
                    .event_sender
                    .send(BluetoothEvent::DiscoveryComplete)
                    .await;
            }

            Event::ConnectionComplete(conn) => {
                let addr_slice = conn.bd_addr.raw();
                let mut addr = [0u8; 6];
                if addr_slice.len() >= 6 {
                    addr.copy_from_slice(&addr_slice[..6]);
                }
                let handle = conn.handle.raw();

                if conn.status == Status::SUCCESS {
                    let _ = self.connections.insert(handle, addr);
                    self.set_state(BluetoothState::Connected).await;
                    let () = self
                        .event_sender
                        .send(BluetoothEvent::DeviceConnected { addr, handle })
                        .await;
                } else {
                    let () = self
                        .event_sender
                        .send(BluetoothEvent::ConnectionFailed {
                            addr,
                            reason: conn.status.into(),
                        })
                        .await;
                    self.set_state(BluetoothState::Idle).await;
                }
            }

            Event::DisconnectionComplete(disc) => {
                let handle = disc.handle.raw();
                if let Some(addr) = self.connections.remove(&handle) {
                    let () = self
                        .event_sender
                        .send(BluetoothEvent::DeviceDisconnected { addr, handle })
                        .await;
                }

                // Update state if no more connections
                if self.connections.is_empty() {
                    self.set_state(BluetoothState::Idle).await;
                }
            }

            // Handle other events as needed
            _ => {
                defmt::trace!("Unhandled HCI event: {:?}", defmt::Debug2Format(event));
            }
        }
    }

    /// Handle commands from `EventHandler`
    async fn handle_command(&mut self, command: BluetoothCommand) -> Result<(), BluetoothError> {
        match command {
            BluetoothCommand::Initialize => self.initialize().await,

            BluetoothCommand::Reset => self.reset().await,

            BluetoothCommand::StartDiscovery { duration_seconds } => {
                self.start_discovery(duration_seconds).await
            }

            BluetoothCommand::StopDiscovery => self.stop_discovery().await,

            BluetoothCommand::Connect {
                addr,
                packet_types,
                page_scan_mode,
                clock_offset,
                allow_role_switch,
            } => {
                self.connect(
                    addr,
                    packet_types,
                    page_scan_mode,
                    clock_offset,
                    allow_role_switch,
                )
                .await
            }

            BluetoothCommand::Disconnect { handle } => self.disconnect(handle).await,

            BluetoothCommand::SetEventMask(mask) => {
                self.set_event_mask(mask);
                Ok(())
            }

            _ => {
                defmt::warn!("Unhandled command: {:?}", defmt::Debug2Format(&command));
                Ok(())
            }
        }
    }

    /// Initialize the Bluetooth stack
    async fn initialize(&mut self) -> Result<(), BluetoothError> {
        self.set_state(BluetoothState::Initializing).await;

        // Reset controller
        self.controller
            .exec(&bt_hci::cmd::controller_baseband::Reset::new())
            .await
            .map_err(|_| BluetoothError::HciError)?;

        // Set default event mask
        self.set_event_mask(0x1FFF);

        self.set_state(BluetoothState::Idle).await;
        Ok(())
    }

    async fn reset(&mut self) -> Result<(), BluetoothError> {
        self.controller
            .exec(&bt_hci::cmd::controller_baseband::Reset::new())
            .await
            .map_err(|_| BluetoothError::HciError)?;

        // Clear state
        self.discovered_devices.clear();
        self.connections.clear();
        self.discovery_active = false;

        self.set_state(BluetoothState::Idle).await;
        Ok(())
    }

    async fn start_discovery(&mut self, duration_seconds: u8) -> Result<(), BluetoothError> {
        if self.state != BluetoothState::Idle {
            return Err(BluetoothError::InvalidState);
        }

        self.discovered_devices.clear();

        let inquiry_cmd = bt_hci::cmd::link_control::Inquiry::new(
            [0x9E, 0x8B, 0x33], // GIAC
            duration_seconds,
            0, // unlimited responses
        );

        self.controller
            .exec(&inquiry_cmd)
            .await
            .map_err(|_| BluetoothError::DiscoveryFailed)?;

        self.discovery_active = true;
        self.set_state(BluetoothState::Discovering).await;
        Ok(())
    }

    async fn stop_discovery(&mut self) -> Result<(), BluetoothError> {
        if !self.discovery_active {
            return Ok(());
        }

        self.discovery_active = false;
        self.set_state(BluetoothState::Idle).await;
        Ok(())
    }

    async fn connect(
        &mut self,
        addr: [u8; 6],
        packet_types: u16,
        page_scan_mode: u8,
        clock_offset: u16,
        allow_role_switch: bool,
    ) -> Result<(), BluetoothError> {
        if self.state != BluetoothState::Idle {
            return Err(BluetoothError::InvalidState);
        }

        let connect_cmd = bt_hci::cmd::link_control::CreateConnection::new(
            bt_hci::param::BdAddr::new(addr),
            packet_types,
            page_scan_mode,
            1,
            clock_offset,
            u8::from(allow_role_switch),
        );

        self.controller
            .exec(&connect_cmd)
            .await
            .map_err(|_| BluetoothError::ConnectionFailed)?;

        self.set_state(BluetoothState::Connecting).await;
        Ok(())
    }

    async fn disconnect(&mut self, handle: u16) -> Result<(), BluetoothError> {
        let disconnect_cmd = bt_hci::cmd::link_control::Disconnect::new(
            ConnHandle::new(handle),
            bt_hci::param::DisconnectReason::RemoteUserTerminatedConn,
        );

        self.controller
            .exec(&disconnect_cmd)
            .await
            .map_err(|_| BluetoothError::HciError)?;

        Ok(())
    }

    #[allow(clippy::unused_self)]
    fn set_event_mask(&mut self, _mask: u64) {
        // let event_mask = bt_hci::param::EventMask::new(mask);

        // self.controller
        //     .exec(&bt_hci::cmd::controller_baseband::SetEventMask::new(
        //         event_mask,
        //     ))
        //     .await
        //     .map_err(|_| BluetoothError::HciError)?;
    }

    async fn set_state(&mut self, new_state: BluetoothState) {
        if self.state != new_state {
            self.state = new_state;
            let () = self
                .event_sender
                .send(BluetoothEvent::StateChanged(new_state))
                .await;
        }
    }
}
