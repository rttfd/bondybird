use bt_hci::{controller::ExternalController, transport::Transport};
use heapless::Vec;

use crate::{
    BluetoothDevice, BluetoothError, BluetoothHost, BluetoothState, Request, Response, constants,
};

impl BluetoothHost {
    /// Process an API request
    pub(crate) async fn process_api_request<T: Transport + 'static, const SLOTS: usize>(
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
                        Response::DiscoverStarted
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
                    self.devices.values().cloned().collect();
                Response::Devices(devices_vec)
            }
            Request::Pair(address) => match self.connect_device(address, controller).await {
                Ok(()) => Response::PairComplete,
                Err(e) => Response::Error(e),
            },
            Request::Disconnect(address) => match self.disconnect_device(address, controller).await
            {
                Ok(()) => Response::DisconnectComplete,
                Err(e) => Response::Error(e),
            },
            Request::GetState => Response::State(self.state),
            Request::GetLocalInfo => Response::LocalInfo(self.local_info),
            Request::GetPairedDevices => {
                // Get devices that have active connections (paired/connected devices)
                let mut paired_devices: Vec<BluetoothDevice, { constants::MAX_CHANNELS }> =
                    Vec::new();

                for (addr, _handle) in &self.connections {
                    if let Some(device) = self.devices.get(addr) {
                        if paired_devices.push(device.clone()).is_err() {
                            // If we can't add more devices, break (shouldn't happen with proper constants)
                            break;
                        }
                    }
                }

                Response::PairedDevices(paired_devices)
            }
            Request::GetDeviceName(address) => {
                match self.get_device_name(address, controller).await {
                    Ok(name) => Response::DeviceName(address, name),
                    Err(e) => Response::Error(e),
                }
            }
        }
    }
}
