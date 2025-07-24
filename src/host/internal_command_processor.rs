use bt_hci::{
    cmd,
    controller::{ControllerCmdSync, ExternalController},
    transport::Transport,
};

use crate::{BluetoothHost, InternalCommand};

impl BluetoothHost {
    /// Process internal command (fire-and-forget, no response needed)
    pub(crate) async fn process_internal_command<T: Transport + 'static, const SLOTS: usize>(
        &mut self,
        command: InternalCommand,
        controller: &ExternalController<T, SLOTS>,
    ) {
        match command {
            InternalCommand::AuthenticationRequested { conn_handle } => {
                let auth_requested = cmd::link_control::AuthenticationRequested::new(
                    bt_hci::param::ConnHandle::new(conn_handle),
                );

                if let Err(e) = controller.exec(&auth_requested).await {
                    defmt::warn!(
                        "Failed to send Authentication_Requested command for handle {}: {:?}",
                        conn_handle,
                        defmt::Debug2Format(&e)
                    );
                } else {
                    defmt::debug!(
                        "Authentication_Requested command sent for connection handle {}",
                        conn_handle
                    );
                }
            }
            InternalCommand::LinkKeyRequestReply { bd_addr, link_key } => {
                let reply = cmd::link_control::LinkKeyRequestReply::new(bd_addr.into(), link_key);

                if let Err(e) = controller.exec(&reply).await {
                    defmt::warn!(
                        "Failed to send Link Key Request Reply for address {}: {:?}",
                        bd_addr,
                        defmt::Debug2Format(&e)
                    );
                } else {
                    defmt::debug!("Link Key Request Reply sent");
                }
            }
            InternalCommand::LinkKeyRequestNegativeReply { bd_addr } => {
                let negative_reply =
                    cmd::link_control::LinkKeyRequestNegativeReply::new(bd_addr.into());

                if let Err(e) = controller.exec(&negative_reply).await {
                    defmt::warn!(
                        "Failed to send Link Key Request Negative Reply for address {}: {:?}",
                        bd_addr,
                        defmt::Debug2Format(&e)
                    );
                } else {
                    defmt::debug!("Link Key Request Negative Reply sent");
                }
            }
            InternalCommand::IoCapabilityRequestReply { bd_addr } => {
                let io_reply = cmd::link_control::IoCapabilityRequestReply::new(
                    bd_addr.into(),
                    bt_hci::param::IoCapability::NoInputNoOutput,
                    bt_hci::param::OobDataPresent::NotPresent,
                    bt_hci::param::AuthenticationRequirements::MitmNotRequiredGeneralBonding,
                );

                if let Err(e) = controller.exec(&io_reply).await {
                    defmt::warn!(
                        "Failed to send IO Capability Request Reply for address {}: {:?}",
                        bd_addr,
                        defmt::Debug2Format(&e)
                    );
                } else {
                    defmt::debug!("IO Capability Request Reply sent");
                }
            }
            InternalCommand::UserConfirmationRequestReply { bd_addr } => {
                let confirmation_reply =
                    cmd::link_control::UserConfirmationRequestReply::new(bd_addr.into());

                if let Err(e) = controller.exec(&confirmation_reply).await {
                    defmt::warn!(
                        "Failed to send User Confirmation Request Reply for address {}: {:?}",
                        bd_addr,
                        defmt::Debug2Format(&e)
                    );
                } else {
                    defmt::debug!("User Confirmation Request Reply sent (auto-accepted)");
                }
            }
            InternalCommand::PinCodeRequestReply { bd_addr, pin_code } => {
                // PIN code length is typically the actual length of the PIN (4 for "0000")
                if let Ok(pin_length) = u8::try_from(
                    pin_code
                        .iter()
                        .position(|&x| x == 0)
                        .unwrap_or(pin_code.len()),
                ) {
                    let pin_reply = cmd::link_control::PinCodeRequestReply::new(
                        bd_addr.into(),
                        pin_length,
                        pin_code,
                    );

                    if let Err(e) = controller.exec(&pin_reply).await {
                        defmt::warn!(
                            "Failed to send PIN Code Request Reply for address {}: {:?}",
                            bd_addr,
                            defmt::Debug2Format(&e)
                        );
                    } else {
                        defmt::debug!("PIN Code Request Reply sent");
                    }
                }
            }
            InternalCommand::UpsertDevice(device) => {
                self.upsert_device(device);
            }
            InternalCommand::SetDiscovering(is_discovering) => {
                self.discovering = is_discovering;
            }
            InternalCommand::SetState(state) => {
                self.state = state;
            }
            InternalCommand::AddConnection(addr, conn_handle) => {
                self.connections.insert(addr, conn_handle).ok();
            }
            InternalCommand::RemoveConnection(conn_handle) => {
                self.remove_connections_by_handle(conn_handle);
            }
            InternalCommand::UpdateDeviceName(addr, name_bytes) => {
                self.update_device_name(addr, name_bytes);
            }
            InternalCommand::StoreLinkKey(addr, link_key) => {
                self.store_link_key(addr, link_key).ok();
            }
            InternalCommand::AddAclConnection(addr, conn_handle) => {
                // Add ACL connection to the ACL manager
                if let Err(e) = self.acl_manager.add_connection(conn_handle, addr) {
                    defmt::warn!("Failed to add ACL connection {}: {}", conn_handle, e);
                } else {
                    defmt::debug!("Added ACL connection {} for {}", conn_handle, addr);
                }
            }
            InternalCommand::RemoveAclConnection(conn_handle) => {
                // Remove ACL connection from the ACL manager
                if let Some(_connection) = self.acl_manager.remove_connection(conn_handle) {
                    defmt::debug!("Removed ACL connection {}", conn_handle);
                }
            }
            InternalCommand::ProcessAclData {
                handle,
                packet_boundary,
                broadcast_flag,
                data,
            } => {
                use crate::acl::{AclHeader, AclPacket, BroadcastFlag, PacketBoundary};

                // Convert raw values back to enum types
                let boundary = match packet_boundary {
                    0 => PacketBoundary::FirstNonFlushable,
                    1 => PacketBoundary::ContinuingFragment,
                    2 => PacketBoundary::FirstFlushable,
                    3 => PacketBoundary::CompletePdu,
                    _ => {
                        defmt::warn!("Invalid packet boundary flag: {}", packet_boundary);
                        return;
                    }
                };

                let broadcast = match broadcast_flag {
                    0 => BroadcastFlag::PointToPoint,
                    1 => BroadcastFlag::ActiveBroadcast,
                    2 => BroadcastFlag::PiconetBroadcast,
                    3 => BroadcastFlag::Reserved,
                    _ => {
                        defmt::warn!("Invalid broadcast flag: {}", broadcast_flag);
                        return;
                    }
                };

                // Create ACL packet and process it
                #[allow(clippy::cast_possible_truncation)]
                let header = AclHeader::new(handle, boundary, broadcast, data.len() as u16);
                let packet = AclPacket::new(header, data);

                if let Err(e) = self.acl_manager.process_acl_packet(&packet) {
                    defmt::error!("Failed to process ACL data for handle {}: {}", handle, e);
                }
            }
        }
    }
}
