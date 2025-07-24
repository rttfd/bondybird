//! L2CAP Signaling Processor
//!
//! This module implements the L2CAP signaling processor that handles
//! signaling commands and manages channel state transitions.

use super::{
    channel::{ChannelManager, ChannelState, ConfigurationOptions, ConfigurationState},
    packet::{ChannelId, L2capError, ProtocolServiceMultiplexer},
    signaling::{
        ConfigurationRequest, ConfigurationResponse, ConfigurationResult, ConnectionRequest,
        ConnectionResponse, ConnectionResult, DisconnectionRequest, DisconnectionResponse,
        SignalingCode, SignalingCommand, SignalingHeader,
    },
};
use crate::BluetoothAddress;
use heapless::Vec;

/// Maximum number of pending signaling requests
pub const MAX_PENDING_REQUESTS: usize = 8;

/// Signaling identifier generator
#[derive(Debug)]
pub struct SignalingIdentifier {
    next_id: u8,
}

impl SignalingIdentifier {
    /// Create new identifier generator
    #[must_use]
    pub fn new() -> Self {
        Self { next_id: 1 }
    }

    /// Generate next identifier (1-255, wrapping around)
    pub fn next_identifier(&mut self) -> u8 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        if self.next_id == 0 {
            self.next_id = 1;
        }
        id
    }
}

impl Default for SignalingIdentifier {
    fn default() -> Self {
        Self::new()
    }
}

/// Pending signaling request information
#[derive(Debug, Clone)]
pub struct PendingRequest {
    /// Request identifier
    pub identifier: u8,
    /// Local channel identifier (if applicable)
    pub local_cid: Option<ChannelId>,
    /// Command code
    pub code: SignalingCode,
}

/// L2CAP Signaling Processor
///
/// Handles L2CAP signaling protocol operations including connection
/// establishment, configuration, and disconnection.
#[derive(Debug)]
pub struct SignalingProcessor {
    /// Channel manager
    channel_manager: ChannelManager,
    /// Identifier generator
    identifier_gen: SignalingIdentifier,
    /// Pending requests
    pending_requests: Vec<PendingRequest, MAX_PENDING_REQUESTS>,
}

impl SignalingProcessor {
    /// Create new signaling processor
    #[must_use]
    pub fn new() -> Self {
        Self {
            channel_manager: ChannelManager::new(),
            identifier_gen: SignalingIdentifier::new(),
            pending_requests: Vec::new(),
        }
    }

    /// Get reference to channel manager
    #[must_use]
    pub fn channel_manager(&self) -> &ChannelManager {
        &self.channel_manager
    }

    /// Get mutable reference to channel manager
    pub fn channel_manager_mut(&mut self) -> &mut ChannelManager {
        &mut self.channel_manager
    }

    /// Initiate outgoing L2CAP connection
    ///
    /// # Errors
    /// Returns `L2capError` if connection cannot be initiated
    pub fn connect(
        &mut self,
        psm: ProtocolServiceMultiplexer,
        remote_addr: BluetoothAddress,
        conn_handle: u16,
    ) -> Result<(ChannelId, SignalingCommand<64>), L2capError> {
        // Create channel
        let local_cid =
            self.channel_manager
                .create_outgoing_channel(psm, remote_addr, conn_handle)?;

        // Update channel state
        if let Some(channel) = self.channel_manager.get_channel_mut(local_cid) {
            channel.set_state(ChannelState::WaitConnect);
        }

        // Generate connection request
        let identifier = self.identifier_gen.next_identifier();
        #[allow(clippy::cast_possible_truncation)]
        let header = SignalingHeader::new(
            SignalingCode::ConnectionRequest,
            identifier,
            ConnectionRequest::SIZE as u16,
        );
        let payload = ConnectionRequest::new(psm, local_cid);
        let command = SignalingCommand::ConnectionRequest { header, payload };

        // Track pending request
        let pending = PendingRequest {
            identifier,
            local_cid: Some(local_cid),
            code: SignalingCode::ConnectionRequest,
        };
        self.pending_requests
            .push(pending)
            .map_err(|_| L2capError::PayloadTooLarge)?;

        Ok((local_cid, command))
    }

    /// Handle incoming signaling command
    ///
    /// # Errors
    /// Returns `L2capError` if command cannot be processed
    pub fn handle_command(
        &mut self,
        command: SignalingCommand<64>,
        remote_addr: BluetoothAddress,
        conn_handle: u16,
    ) -> Result<Option<SignalingCommand<64>>, L2capError> {
        match command {
            SignalingCommand::ConnectionRequest { header, payload } => {
                self.handle_connection_request(header, payload, remote_addr, conn_handle)
            }
            SignalingCommand::ConnectionResponse { header, payload } => {
                self.handle_connection_response(header, payload)?;
                Ok(None)
            }
            SignalingCommand::ConfigurationRequest { header, payload } => {
                Ok(self.handle_configuration_request(header, payload, conn_handle))
            }
            SignalingCommand::ConfigurationResponse { header, payload } => {
                self.handle_configuration_response(header, &payload)?;
                Ok(None)
            }
            SignalingCommand::DisconnectionRequest { header, payload } => {
                Ok(self.handle_disconnection_request(header, payload, conn_handle))
            }
            SignalingCommand::DisconnectionResponse { header, payload } => {
                self.handle_disconnection_response(header, payload)?;
                Ok(None)
            }
            SignalingCommand::Unknown { .. } => {
                // Ignore unknown commands for forward compatibility
                Ok(None)
            }
        }
    }

    /// Handle connection request command
    fn handle_connection_request(
        &mut self,
        header: SignalingHeader,
        payload: ConnectionRequest,
        remote_addr: BluetoothAddress,
        conn_handle: u16,
    ) -> Result<Option<SignalingCommand<64>>, L2capError> {
        // Check if PSM is supported (for now, only AVDTP)
        if payload.psm != super::packet::psm::AVDTP {
            #[allow(clippy::cast_possible_truncation)]
            let response_header = SignalingHeader::new(
                SignalingCode::ConnectionResponse,
                header.identifier,
                ConnectionResponse::SIZE as u16,
            );
            let response_payload =
                ConnectionResponse::reject(payload.source_cid, ConnectionResult::PsmNotSupported);
            return Ok(Some(SignalingCommand::ConnectionResponse {
                header: response_header,
                payload: response_payload,
            }));
        }

        // Create incoming channel
        let local_cid = self.channel_manager.create_incoming_channel(
            payload.source_cid,
            payload.psm,
            remote_addr,
            conn_handle,
        )?;

        // Generate success response
        #[allow(clippy::cast_possible_truncation)]
        let response_header = SignalingHeader::new(
            SignalingCode::ConnectionResponse,
            header.identifier,
            ConnectionResponse::SIZE as u16,
        );
        let response_payload = ConnectionResponse::success(local_cid, payload.source_cid);

        Ok(Some(SignalingCommand::ConnectionResponse {
            header: response_header,
            payload: response_payload,
        }))
    }

    /// Handle connection response command
    fn handle_connection_response(
        &mut self,
        header: SignalingHeader,
        payload: ConnectionResponse,
    ) -> Result<(), L2capError> {
        // Find and remove pending request
        let pending_idx = self
            .pending_requests
            .iter()
            .position(|req| {
                req.identifier == header.identifier && req.code == SignalingCode::ConnectionRequest
            })
            .ok_or(L2capError::InsufficientData)?;

        let pending = self.pending_requests.swap_remove(pending_idx);
        let local_cid = pending.local_cid.ok_or(L2capError::InsufficientData)?;

        // Update channel based on result
        if let Some(channel) = self.channel_manager.get_channel_mut(local_cid) {
            match payload.result {
                ConnectionResult::Success => {
                    channel.set_remote_cid(payload.destination_cid);
                    channel.set_state(ChannelState::Config);
                }
                _ => {
                    // Connection failed, clean up
                    channel.set_state(ChannelState::Closed);
                }
            }
        }

        Ok(())
    }

    /// Handle configuration request command
    #[allow(clippy::unused_self, clippy::unnecessary_wraps)]
    fn handle_configuration_request(
        &mut self,
        header: SignalingHeader,
        _payload: ConfigurationRequest<64>,
        _conn_handle: u16,
    ) -> Option<SignalingCommand<64>> {
        // For now, accept all configuration requests with default settings
        #[allow(clippy::cast_possible_truncation)]
        let response_header = SignalingHeader::new(
            SignalingCode::ConfigurationResponse,
            header.identifier,
            ConfigurationResponse::<64>::MIN_SIZE as u16,
        );
        let response_payload = ConfigurationResponse::success(u16::from(header.identifier), 0);

        Some(SignalingCommand::ConfigurationResponse {
            header: response_header,
            payload: response_payload,
        })
    }

    /// Handle configuration response command
    fn handle_configuration_response(
        &mut self,
        header: SignalingHeader,
        payload: &ConfigurationResponse<64>,
    ) -> Result<(), L2capError> {
        // Find and remove pending request
        let pending_idx = self
            .pending_requests
            .iter()
            .position(|req| {
                req.identifier == header.identifier
                    && req.code == SignalingCode::ConfigurationRequest
            })
            .ok_or(L2capError::InsufficientData)?;

        let pending = self.pending_requests.swap_remove(pending_idx);
        let local_cid = pending.local_cid.ok_or(L2capError::InsufficientData)?;

        // Update channel configuration status
        if let Some(channel) = self.channel_manager.get_channel_mut(local_cid) {
            match payload.result {
                ConfigurationResult::Success => {
                    channel.config_flags.local_state = ConfigurationState::Complete;

                    // Check if both sides are configured
                    if channel.is_configured() {
                        channel.set_state(ChannelState::Open);
                    }
                }
                _ => {
                    // Configuration failed
                    channel.set_state(ChannelState::Closed);
                }
            }
        }

        Ok(())
    }

    /// Configure channel after connection established
    ///
    /// # Errors
    /// Returns `L2capError` if configuration cannot be initiated
    pub fn configure_channel(
        &mut self,
        local_cid: ChannelId,
        options: ConfigurationOptions,
    ) -> Result<SignalingCommand<64>, L2capError> {
        let channel = self
            .channel_manager
            .get_channel_mut(local_cid)
            .ok_or(L2capError::InsufficientData)?;

        let remote_cid = channel.remote_cid.ok_or(L2capError::InsufficientData)?;

        // Update local configuration
        channel.local_config = options;
        channel.config_flags.local_state = ConfigurationState::InProgress;

        // Generate configuration request
        let identifier = self.identifier_gen.next_identifier();
        let payload = ConfigurationRequest::new(remote_cid, 0, Vec::new());
        #[allow(clippy::cast_possible_truncation)]
        let header = SignalingHeader::new(
            SignalingCode::ConfigurationRequest,
            identifier,
            ConfigurationRequest::<64>::MIN_SIZE as u16,
        );
        let command = SignalingCommand::ConfigurationRequest { header, payload };

        // Track pending request
        let pending = PendingRequest {
            identifier,
            local_cid: Some(local_cid),
            code: SignalingCode::ConfigurationRequest,
        };
        self.pending_requests
            .push(pending)
            .map_err(|_| L2capError::PayloadTooLarge)?;

        Ok(command)
    }

    /// Handle disconnection request command
    #[allow(clippy::unnecessary_wraps)]
    fn handle_disconnection_request(
        &mut self,
        header: SignalingHeader,
        payload: DisconnectionRequest,
        conn_handle: u16,
    ) -> Option<SignalingCommand<64>> {
        // Find channel by remote CID
        if let Some(channel) = self
            .channel_manager
            .find_by_remote_cid(payload.source_cid, conn_handle)
        {
            let local_cid = channel.local_cid;

            // Remove the channel
            self.channel_manager.remove_channel(local_cid);

            // Generate response
            #[allow(clippy::cast_possible_truncation)]
            let response_header = SignalingHeader::new(
                SignalingCode::DisconnectionResponse,
                header.identifier,
                DisconnectionResponse::SIZE as u16,
            );
            let response_payload =
                DisconnectionResponse::new(payload.source_cid, payload.destination_cid);

            Some(SignalingCommand::DisconnectionResponse {
                header: response_header,
                payload: response_payload,
            })
        } else {
            // Channel not found - this is an error condition but we still respond
            #[allow(clippy::cast_possible_truncation)]
            let response_header = SignalingHeader::new(
                SignalingCode::DisconnectionResponse,
                header.identifier,
                DisconnectionResponse::SIZE as u16,
            );
            let response_payload =
                DisconnectionResponse::new(payload.source_cid, payload.destination_cid);

            Some(SignalingCommand::DisconnectionResponse {
                header: response_header,
                payload: response_payload,
            })
        }
    }

    /// Handle disconnection response command
    fn handle_disconnection_response(
        &mut self,
        header: SignalingHeader,
        _payload: DisconnectionResponse,
    ) -> Result<(), L2capError> {
        // Find and remove pending request
        let pending_idx = self
            .pending_requests
            .iter()
            .position(|req| {
                req.identifier == header.identifier
                    && req.code == SignalingCode::DisconnectionRequest
            })
            .ok_or(L2capError::InsufficientData)?;

        let pending = self.pending_requests.swap_remove(pending_idx);

        // Remove the channel if it exists
        if let Some(local_cid) = pending.local_cid {
            self.channel_manager.remove_channel(local_cid);
        }

        Ok(())
    }

    /// Initiate channel disconnection
    ///
    /// # Errors
    /// Returns `L2capError` if disconnection cannot be initiated
    pub fn disconnect(&mut self, local_cid: ChannelId) -> Result<SignalingCommand<64>, L2capError> {
        let channel = self
            .channel_manager
            .get_channel_mut(local_cid)
            .ok_or(L2capError::InsufficientData)?;

        let remote_cid = channel.remote_cid.ok_or(L2capError::InsufficientData)?;

        // Update channel state
        channel.set_state(ChannelState::WaitDisconnect);

        // Generate disconnection request
        let identifier = self.identifier_gen.next_identifier();
        #[allow(clippy::cast_possible_truncation)]
        let header = SignalingHeader::new(
            SignalingCode::DisconnectionRequest,
            identifier,
            DisconnectionRequest::SIZE as u16,
        );
        let payload = DisconnectionRequest::new(remote_cid, local_cid);
        let command = SignalingCommand::DisconnectionRequest { header, payload };

        // Track pending request
        let pending = PendingRequest {
            identifier,
            local_cid: Some(local_cid),
            code: SignalingCode::DisconnectionRequest,
        };
        self.pending_requests
            .push(pending)
            .map_err(|_| L2capError::PayloadTooLarge)?;

        Ok(command)
    }

    /// Get number of pending requests
    #[must_use]
    pub fn pending_request_count(&self) -> usize {
        self.pending_requests.len()
    }
}

impl Default for SignalingProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::super::packet::{cid, psm};
    use super::*;

    #[test]
    fn test_identifier_generation() {
        let mut generator = SignalingIdentifier::new();

        let id1 = generator.next_identifier();
        let id2 = generator.next_identifier();
        let id3 = generator.next_identifier();

        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
        assert_eq!(id3, 3);
        assert_ne!(id1, id2);
        assert_ne!(id2, id3);
    }

    #[test]
    fn test_identifier_wrapping() {
        let mut generator = SignalingIdentifier { next_id: 255 };

        let id1 = generator.next_identifier();
        let id2 = generator.next_identifier();

        assert_eq!(id1, 255);
        assert_eq!(id2, 1); // Should wrap to 1, not 0
    }

    #[test]
    fn test_connect_initiation() {
        let mut processor = SignalingProcessor::new();
        let remote_addr = BluetoothAddress::new([0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC]);

        let result = processor.connect(psm::AVDTP, remote_addr, 0x0001);
        assert!(result.is_ok());

        let (local_cid, command) = result.unwrap();
        assert!(local_cid >= cid::DYNAMIC_START);
        assert_eq!(processor.pending_request_count(), 1);

        match command {
            SignalingCommand::ConnectionRequest { header, payload } => {
                assert_eq!(header.code, SignalingCode::ConnectionRequest);
                assert_eq!(payload.psm, psm::AVDTP);
                assert_eq!(payload.source_cid, local_cid);
            }
            _ => panic!("Wrong command type generated"),
        }
    }

    #[test]
    fn test_connection_request_handling() {
        let mut processor = SignalingProcessor::new();
        let remote_addr = BluetoothAddress::new([0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC]);

        #[allow(clippy::cast_possible_truncation)]
        let header = SignalingHeader::new(
            SignalingCode::ConnectionRequest,
            0x42,
            ConnectionRequest::SIZE as u16,
        );
        let payload = ConnectionRequest::new(psm::AVDTP, 0x0050);
        let command = SignalingCommand::ConnectionRequest { header, payload };

        let result = processor.handle_command(command, remote_addr, 0x0001);
        assert!(result.is_ok());

        let response = result.unwrap();
        assert!(response.is_some());

        match response.unwrap() {
            SignalingCommand::ConnectionResponse {
                header: h,
                payload: p,
            } => {
                assert_eq!(h.identifier, 0x42);
                assert_eq!(p.result, ConnectionResult::Success);
                assert_eq!(p.source_cid, 0x0050);
            }
            _ => panic!("Wrong response type generated"),
        }
    }

    #[test]
    fn test_unsupported_psm_rejection() {
        let mut processor = SignalingProcessor::new();
        let remote_addr = BluetoothAddress::new([0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC]);

        #[allow(clippy::cast_possible_truncation)]
        let header = SignalingHeader::new(
            SignalingCode::ConnectionRequest,
            0x42,
            ConnectionRequest::SIZE as u16,
        );
        let payload = ConnectionRequest::new(0x1234, 0x0050); // Unsupported PSM
        let command = SignalingCommand::ConnectionRequest { header, payload };

        let result = processor.handle_command(command, remote_addr, 0x0001);
        assert!(result.is_ok());

        let response = result.unwrap();
        assert!(response.is_some());

        match response.unwrap() {
            SignalingCommand::ConnectionResponse { payload: p, .. } => {
                assert_eq!(p.result, ConnectionResult::PsmNotSupported);
                assert_eq!(p.source_cid, 0x0050);
            }
            _ => panic!("Wrong response type generated"),
        }
    }

    #[test]
    fn test_channel_configuration() {
        let mut processor = SignalingProcessor::new();
        let remote_addr = BluetoothAddress::new([0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC]);

        // First establish connection
        let (local_cid, _) = processor.connect(psm::AVDTP, remote_addr, 0x0001).unwrap();

        // Simulate successful connection response
        #[allow(clippy::cast_possible_truncation)]
        let conn_resp_header = SignalingHeader::new(
            SignalingCode::ConnectionResponse,
            1,
            ConnectionResponse::SIZE as u16,
        );
        let conn_resp_payload = ConnectionResponse::success(0x0050, local_cid);
        let conn_resp_command = SignalingCommand::ConnectionResponse {
            header: conn_resp_header,
            payload: conn_resp_payload,
        };
        processor
            .handle_command(conn_resp_command, remote_addr, 0x0001)
            .unwrap();

        // Now configure the channel
        let config_options = ConfigurationOptions::default();
        let result = processor.configure_channel(local_cid, config_options);
        assert!(result.is_ok());

        let command = result.unwrap();
        match command {
            SignalingCommand::ConfigurationRequest { header, payload } => {
                assert_eq!(header.code, SignalingCode::ConfigurationRequest);
                assert_eq!(payload.destination_cid, 0x0050);
            }
            _ => panic!("Wrong command type generated"),
        }
    }

    #[test]
    fn test_disconnection() {
        let mut processor = SignalingProcessor::new();
        let remote_addr = BluetoothAddress::new([0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC]);

        // Establish connection first
        let (local_cid, _) = processor.connect(psm::AVDTP, remote_addr, 0x0001).unwrap();

        // Simulate successful connection response
        #[allow(clippy::cast_possible_truncation)]
        let conn_resp_header = SignalingHeader::new(
            SignalingCode::ConnectionResponse,
            1,
            ConnectionResponse::SIZE as u16,
        );
        let conn_resp_payload = ConnectionResponse::success(0x0050, local_cid);
        let conn_resp_command = SignalingCommand::ConnectionResponse {
            header: conn_resp_header,
            payload: conn_resp_payload,
        };
        processor
            .handle_command(conn_resp_command, remote_addr, 0x0001)
            .unwrap();

        // Now disconnect
        let result = processor.disconnect(local_cid);
        assert!(result.is_ok());

        let command = result.unwrap();
        match command {
            SignalingCommand::DisconnectionRequest { header, payload } => {
                assert_eq!(header.code, SignalingCode::DisconnectionRequest);
                assert_eq!(payload.destination_cid, 0x0050);
                assert_eq!(payload.source_cid, local_cid);
            }
            _ => panic!("Wrong command type generated"),
        }

        assert_eq!(processor.pending_request_count(), 1);
    }
}
