// =============================================================================
// EVENT HANDLER (BUSINESS LOGIC)
// =============================================================================

use embassy_time::{Duration, Timer};
use heapless::Vec;

use crate::{
    ApiCommand, ApiCommandReceiver, ApiResponse, ApiResponseSender, BluetoothCommand,
    BluetoothDevice, BluetoothEvent, BluetoothState, CommandSender, EventReceiver, MAX_CONNECTIONS,
    MAX_DISCOVERED_DEVICES,
};

/// Event Handler - Receives events and implements business logic
pub struct BluetoothEventHandler {
    // Communication channels
    event_receiver: EventReceiver,
    command_sender: CommandSender,

    api_command_receiver: ApiCommandReceiver,
    api_response_sender: ApiResponseSender,

    // Application state
    state: BluetoothState,
    discovered_devices: Vec<BluetoothDevice, MAX_DISCOVERED_DEVICES>,
    connected_devices: Vec<([u8; 6], u16), MAX_CONNECTIONS>, // (addr, handle)

    // Configuration
    auto_connect_enabled: bool,
    target_device: Option<[u8; 6]>,
    connection_attempts: u32,
    max_connection_attempts: u32,
}

impl BluetoothEventHandler {
    /// Create a new Event Handler
    #[must_use]
    pub fn new(
        event_receiver: EventReceiver,
        command_sender: CommandSender,
        api_command_receiver: ApiCommandReceiver,
        api_response_sender: ApiResponseSender,
    ) -> Self {
        Self {
            event_receiver,
            command_sender,
            api_command_receiver,
            api_response_sender,
            state: BluetoothState::PoweredOff,
            discovered_devices: Vec::new(),
            connected_devices: Vec::new(),
            auto_connect_enabled: false,
            target_device: None,
            connection_attempts: 0,
            max_connection_attempts: 3,
        }
    }

    /// Configure auto-connect to a specific device
    pub fn set_auto_connect(&mut self, target: [u8; 6], enabled: bool) {
        self.target_device = if enabled { Some(target) } else { None };
        self.auto_connect_enabled = enabled;
        self.connection_attempts = 0;
    }

    /// Get discovered devices
    #[must_use]
    pub fn discovered_devices(&self) -> &[BluetoothDevice] {
        &self.discovered_devices
    }

    /// Get connected devices
    #[must_use]
    pub fn connected_devices(&self) -> &[([u8; 6], u16)] {
        &self.connected_devices
    }

    /// Get current state
    #[must_use]
    pub fn state(&self) -> BluetoothState {
        self.state
    }

    /// Main event handler loop
    pub async fn run(&mut self) {
        use embassy_futures::select::{Either, select};

        defmt::info!("BluetoothEventHandler starting");

        // Signal that handler is ready
        let () = self.command_sender.send(BluetoothCommand::Initialize).await;

        loop {
            let event_fut = self.event_receiver.receive();
            let api_fut = self.api_command_receiver.receive();

            match select(event_fut, api_fut).await {
                Either::First(event) => {
                    defmt::info!("Received event: {:?}", defmt::Debug2Format(&event));
                    self.handle_event(event).await;
                }
                Either::Second((command, request_id)) => {
                    defmt::info!("Received API command: {:?}", defmt::Debug2Format(&command));
                    let response = self.handle_api_command(command).await;
                    let () = self.api_response_sender.send((response, request_id)).await;
                }
            }
        }
    }

    async fn handle_api_command(&mut self, command: ApiCommand) -> ApiResponse {
        match command {
            ApiCommand::StartDiscovery { duration } => {
                let () = self
                    .command_sender
                    .send(BluetoothCommand::StartDiscovery {
                        duration_seconds: duration,
                    })
                    .await;
                ApiResponse::Success
            }

            ApiCommand::StopDiscovery => {
                let () = self
                    .command_sender
                    .send(BluetoothCommand::StopDiscovery)
                    .await;
                ApiResponse::Success
            }

            ApiCommand::GetDiscoveredDevices => {
                let devices = self.discovered_devices.clone();
                ApiResponse::DiscoveredDevices { devices }
            }

            ApiCommand::ConnectDevice { addr } => {
                self.connect_to_device(addr).await;
                ApiResponse::Success
            }

            ApiCommand::DisconnectDevice { addr } => {
                self.disconnect_device(addr).await;
                ApiResponse::Success
            }

            ApiCommand::GetConnectedDevices => {
                let devices = self.connected_devices.clone();
                ApiResponse::ConnectedDevices { devices }
            }

            ApiCommand::SetAutoConnect { addr, enabled } => {
                self.set_auto_connect(addr, enabled);
                ApiResponse::Success
            }

            ApiCommand::GetStatus => ApiResponse::Status {
                state: self.state(),
                discovered_count: self.discovered_devices().len(),
                connected_count: self.connected_devices().len(),
            },

            ApiCommand::Reset => {
                let () = self.command_sender.send(BluetoothCommand::Reset).await;
                ApiResponse::Success
            }

            ApiCommand::SendData { addr, data } => {
                self.send_data(addr, data).await;
                ApiResponse::DataSent
            }
        }
    }

    /// Handle incoming events
    #[allow(clippy::too_many_lines)]
    async fn handle_event(&mut self, event: BluetoothEvent) {
        match event {
            BluetoothEvent::ManagerReady => {
                defmt::info!("Manager ready, initializing...");
                let () = self.command_sender.send(BluetoothCommand::Initialize).await;
            }

            BluetoothEvent::StateChanged(new_state) => {
                defmt::info!(
                    "State changed: {:?} -> {:?}",
                    defmt::Debug2Format(&self.state),
                    defmt::Debug2Format(&new_state)
                );
                self.state = new_state;

                // Start discovery when idle
                if new_state == BluetoothState::Idle && self.auto_connect_enabled {
                    self.start_discovery().await;
                }
            }

            BluetoothEvent::DeviceDiscovered(device) => {
                defmt::info!("Device discovered: {:?}", device.addr);

                // Store device
                if self.discovered_devices.push(device).is_err() {
                    defmt::warn!("Device list full, removing oldest");
                    self.discovered_devices.remove(0);
                    let _ = self.discovered_devices.push(device);
                }

                // Check for auto-connect
                if self.should_auto_connect(&device) {
                    defmt::info!("Auto-connecting to target device");
                    self.connect_to_device(device.addr).await;
                }
            }

            BluetoothEvent::DiscoveryComplete => {
                defmt::info!(
                    "Discovery complete, found {} devices",
                    self.discovered_devices.len()
                );

                // If auto-connect is enabled but no target found, try again
                if self.auto_connect_enabled
                    && self.connected_devices.is_empty()
                    && self.connection_attempts < self.max_connection_attempts
                {
                    defmt::info!("Target not found, retrying discovery...");
                    Timer::after(Duration::from_secs(2)).await;
                    self.start_discovery().await;
                }
            }

            BluetoothEvent::DeviceConnected { addr, handle } => {
                defmt::info!("Device connected: {:?} (handle: {})", addr, handle);

                // Store connection
                if self.connected_devices.push((addr, handle)).is_err() {
                    defmt::warn!("Connection list full");
                }

                self.connection_attempts = 0; // Reset on success
            }

            BluetoothEvent::DeviceDisconnected { addr, handle } => {
                defmt::info!("Device disconnected: {:?} (handle: {})", addr, handle);

                // Remove from connected list
                if let Some(pos) = self
                    .connected_devices
                    .iter()
                    .position(|(a, h)| *a == addr && *h == handle)
                {
                    self.connected_devices.swap_remove(pos);
                }

                // Attempt reconnection if auto-connect is enabled
                if self.should_auto_connect(&BluetoothDevice {
                    addr,
                    rssi: None,
                    name: None,
                    class_of_device: None,
                }) {
                    self.connection_attempts += 1;
                    if self.connection_attempts < self.max_connection_attempts {
                        defmt::info!(
                            "Attempting reconnection ({}/{})",
                            self.connection_attempts,
                            self.max_connection_attempts
                        );
                        Timer::after(Duration::from_secs(1)).await;
                        self.connect_to_device(addr).await;
                    } else {
                        defmt::warn!("Max reconnection attempts reached");
                    }
                }
            }

            BluetoothEvent::ConnectionFailed { addr, reason } => {
                defmt::error!("Connection failed to {:?}, reason: {}", addr, reason);

                self.connection_attempts += 1;
                if self.auto_connect_enabled
                    && self.connection_attempts < self.max_connection_attempts
                {
                    defmt::info!(
                        "Retrying connection ({}/{})",
                        self.connection_attempts,
                        self.max_connection_attempts
                    );
                    Timer::after(Duration::from_secs(2)).await;
                    self.connect_to_device(addr).await;
                }
            }

            BluetoothEvent::DataReceived { from, handle, data } => {
                defmt::info!(
                    "Data received from {:?} (handle: {}): {} bytes",
                    from,
                    handle,
                    data.len()
                );
                // Process received data here
            }

            BluetoothEvent::Error(error) => {
                defmt::error!("Bluetooth error: {:?}", defmt::Debug2Format(&error));
            }

            BluetoothEvent::HandlerReady => {
                // This event is sent by the handler itself, ignore
            }
        }
    }

    /// Start device discovery
    async fn start_discovery(&mut self) {
        let () = self
            .command_sender
            .send(BluetoothCommand::StartDiscovery {
                duration_seconds: 10,
            })
            .await;
    }

    /// Connect to a device
    async fn connect_to_device(&mut self, addr: [u8; 6]) {
        let () = self
            .command_sender
            .send(BluetoothCommand::Connect {
                addr,
                packet_types: 0xCC18, // DM1, DH1, DM3, DH3, DM5, DH5
                page_scan_mode: 0x01,
                clock_offset: 0x0000,
                allow_role_switch: true,
            })
            .await;
    }

    /// Check if should auto-connect to this device
    fn should_auto_connect(&self, device: &BluetoothDevice) -> bool {
        self.auto_connect_enabled
            && (self.target_device == Some(device.addr))
            && !self
                .connected_devices
                .iter()
                .any(|(addr, _)| *addr == device.addr)
    }

    /// Disconnect from a device
    pub async fn disconnect_device(&mut self, addr: [u8; 6]) {
        if let Some((_, handle)) = self.connected_devices.iter().find(|(a, _)| *a == addr) {
            let () = self
                .command_sender
                .send(BluetoothCommand::Disconnect { handle: *handle })
                .await;
        }
    }

    /// Send data to a connected device
    pub async fn send_data(&mut self, addr: [u8; 6], data: Vec<u8, 64>) {
        if let Some((_, handle)) = self.connected_devices.iter().find(|(a, _)| *a == addr) {
            let () = self
                .command_sender
                .send(BluetoothCommand::SendData {
                    handle: *handle,
                    data,
                })
                .await;
        }
    }
}
