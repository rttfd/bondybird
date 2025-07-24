//! L2CAP Signaling Protocol
//!
//! This module implements the L2CAP signaling protocol for connection-oriented channels.
//! It handles signaling commands like Connect Request/Response, Configure Request/Response,
//! and Disconnect Request/Response as defined in the Bluetooth Core Specification.

use super::packet::{ChannelId, L2capError, ProtocolServiceMultiplexer};
use heapless::Vec;

/// L2CAP Signaling Command Codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SignalingCode {
    /// Command Reject
    CommandReject = 0x01,
    /// Connection Request
    ConnectionRequest = 0x02,
    /// Connection Response
    ConnectionResponse = 0x03,
    /// Configuration Request
    ConfigurationRequest = 0x04,
    /// Configuration Response
    ConfigurationResponse = 0x05,
    /// Disconnection Request
    DisconnectionRequest = 0x06,
    /// Disconnection Response
    DisconnectionResponse = 0x07,
    /// Echo Request
    EchoRequest = 0x08,
    /// Echo Response
    EchoResponse = 0x09,
    /// Information Request
    InformationRequest = 0x0A,
    /// Information Response
    InformationResponse = 0x0B,
}

impl SignalingCode {
    /// Convert from raw byte value
    #[must_use]
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0x01 => Some(Self::CommandReject),
            0x02 => Some(Self::ConnectionRequest),
            0x03 => Some(Self::ConnectionResponse),
            0x04 => Some(Self::ConfigurationRequest),
            0x05 => Some(Self::ConfigurationResponse),
            0x06 => Some(Self::DisconnectionRequest),
            0x07 => Some(Self::DisconnectionResponse),
            0x08 => Some(Self::EchoRequest),
            0x09 => Some(Self::EchoResponse),
            0x0A => Some(Self::InformationRequest),
            0x0B => Some(Self::InformationResponse),
            _ => None,
        }
    }
}

/// L2CAP Signaling Command Header
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SignalingHeader {
    /// Command code
    pub code: SignalingCode,
    /// Command identifier (for matching requests/responses)
    pub identifier: u8,
    /// Length of command data
    pub length: u16,
}

impl SignalingHeader {
    /// Create a new signaling header
    #[must_use]
    pub fn new(code: SignalingCode, identifier: u8, length: u16) -> Self {
        Self {
            code,
            identifier,
            length,
        }
    }

    /// Parse signaling header from bytes
    ///
    /// # Errors
    /// Returns `L2capError::InsufficientData` if not enough bytes or invalid command code
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, L2capError> {
        if bytes.len() < 4 {
            return Err(L2capError::InsufficientData);
        }

        let code = SignalingCode::from_u8(bytes[0]).ok_or(L2capError::InsufficientData)?;
        let identifier = bytes[1];
        let length = u16::from_le_bytes([bytes[2], bytes[3]]);

        Ok(Self::new(code, identifier, length))
    }

    /// Convert header to bytes
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn to_bytes(self) -> [u8; 4] {
        [
            self.code as u8,
            self.identifier,
            self.length as u8,
            (self.length >> 8) as u8,
        ]
    }

    /// Size of signaling header in bytes
    pub const SIZE: usize = 4;
}

/// Connection Request/Response Result Codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum ConnectionResult {
    /// Connection successful
    Success = 0x0000,
    /// Connection pending
    Pending = 0x0001,
    /// Connection refused - PSM not supported
    PsmNotSupported = 0x0002,
    /// Connection refused - security block
    SecurityBlock = 0x0003,
    /// Connection refused - no resources available
    NoResources = 0x0004,
    /// Connection refused - invalid source CID
    InvalidSourceCid = 0x0006,
    /// Connection refused - source CID already allocated
    SourceCidAlreadyAllocated = 0x0007,
}

impl ConnectionResult {
    /// Convert from raw u16 value
    #[must_use]
    pub fn from_u16(value: u16) -> Option<Self> {
        match value {
            0x0000 => Some(Self::Success),
            0x0001 => Some(Self::Pending),
            0x0002 => Some(Self::PsmNotSupported),
            0x0003 => Some(Self::SecurityBlock),
            0x0004 => Some(Self::NoResources),
            0x0006 => Some(Self::InvalidSourceCid),
            0x0007 => Some(Self::SourceCidAlreadyAllocated),
            _ => None,
        }
    }
}

/// Configuration Result Codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum ConfigurationResult {
    /// Success
    Success = 0x0000,
    /// Failure - unacceptable parameters
    UnacceptableParameters = 0x0001,
    /// Failure - rejected (no reason provided)
    Rejected = 0x0002,
    /// Failure - unknown options
    UnknownOptions = 0x0003,
    /// Pending
    Pending = 0x0004,
    /// Failure - flow spec rejected
    FlowSpecRejected = 0x0005,
}

impl ConfigurationResult {
    /// Convert from raw u16 value
    #[must_use]
    pub fn from_u16(value: u16) -> Option<Self> {
        match value {
            0x0000 => Some(Self::Success),
            0x0001 => Some(Self::UnacceptableParameters),
            0x0002 => Some(Self::Rejected),
            0x0003 => Some(Self::UnknownOptions),
            0x0004 => Some(Self::Pending),
            0x0005 => Some(Self::FlowSpecRejected),
            _ => None,
        }
    }
}

/// L2CAP Connection Request
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectionRequest {
    /// Protocol Service Multiplexer
    pub psm: ProtocolServiceMultiplexer,
    /// Source Channel Identifier
    pub source_cid: ChannelId,
}

impl ConnectionRequest {
    /// Create new connection request
    #[must_use]
    pub fn new(psm: ProtocolServiceMultiplexer, source_cid: ChannelId) -> Self {
        Self { psm, source_cid }
    }

    /// Parse from bytes
    ///
    /// # Errors
    /// Returns `L2capError::InsufficientData` if not enough bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, L2capError> {
        if bytes.len() < 4 {
            return Err(L2capError::InsufficientData);
        }

        let psm = u16::from_le_bytes([bytes[0], bytes[1]]);
        let source_cid = u16::from_le_bytes([bytes[2], bytes[3]]);

        Ok(Self::new(psm, source_cid))
    }

    /// Convert to bytes
    #[must_use]
    pub fn to_bytes(self) -> [u8; 4] {
        let mut bytes = [0u8; 4];
        bytes[0..2].copy_from_slice(&self.psm.to_le_bytes());
        bytes[2..4].copy_from_slice(&self.source_cid.to_le_bytes());
        bytes
    }

    /// Size in bytes
    pub const SIZE: usize = 4;
}

/// L2CAP Connection Response
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectionResponse {
    /// Destination Channel Identifier
    pub destination_cid: ChannelId,
    /// Source Channel Identifier
    pub source_cid: ChannelId,
    /// Connection result
    pub result: ConnectionResult,
    /// Connection status (for pending results)
    pub status: u16,
}

impl ConnectionResponse {
    /// Create new connection response
    #[must_use]
    pub fn new(
        destination_cid: ChannelId,
        source_cid: ChannelId,
        result: ConnectionResult,
        status: u16,
    ) -> Self {
        Self {
            destination_cid,
            source_cid,
            result,
            status,
        }
    }

    /// Create success response
    #[must_use]
    pub fn success(destination_cid: ChannelId, source_cid: ChannelId) -> Self {
        Self::new(destination_cid, source_cid, ConnectionResult::Success, 0)
    }

    /// Create rejection response
    #[must_use]
    pub fn reject(source_cid: ChannelId, result: ConnectionResult) -> Self {
        Self::new(0, source_cid, result, 0)
    }

    /// Parse from bytes
    ///
    /// # Errors
    /// Returns `L2capError::InsufficientData` if not enough bytes or invalid result code
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, L2capError> {
        if bytes.len() < 8 {
            return Err(L2capError::InsufficientData);
        }

        let destination_cid = u16::from_le_bytes([bytes[0], bytes[1]]);
        let source_cid = u16::from_le_bytes([bytes[2], bytes[3]]);
        let result_code = u16::from_le_bytes([bytes[4], bytes[5]]);
        let status = u16::from_le_bytes([bytes[6], bytes[7]]);

        let result = ConnectionResult::from_u16(result_code).ok_or(L2capError::InsufficientData)?;

        Ok(Self::new(destination_cid, source_cid, result, status))
    }

    /// Convert to bytes
    #[must_use]
    pub fn to_bytes(self) -> [u8; 8] {
        let mut bytes = [0u8; 8];
        bytes[0..2].copy_from_slice(&self.destination_cid.to_le_bytes());
        bytes[2..4].copy_from_slice(&self.source_cid.to_le_bytes());
        bytes[4..6].copy_from_slice(&(self.result as u16).to_le_bytes());
        bytes[6..8].copy_from_slice(&self.status.to_le_bytes());
        bytes
    }

    /// Size in bytes
    pub const SIZE: usize = 8;
}

/// L2CAP Configuration Request
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigurationRequest<const N: usize = 64> {
    /// Destination Channel Identifier
    pub destination_cid: ChannelId,
    /// Configuration flags
    pub flags: u16,
    /// Configuration options
    pub options: Vec<u8, N>,
}

impl<const N: usize> ConfigurationRequest<N> {
    /// Create new configuration request
    #[must_use]
    pub fn new(destination_cid: ChannelId, flags: u16, options: Vec<u8, N>) -> Self {
        Self {
            destination_cid,
            flags,
            options,
        }
    }

    /// Parse from bytes
    ///
    /// # Errors
    /// Returns `L2capError` if not enough bytes or payload too large
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, L2capError> {
        if bytes.len() < 4 {
            return Err(L2capError::InsufficientData);
        }

        let destination_cid = u16::from_le_bytes([bytes[0], bytes[1]]);
        let flags = u16::from_le_bytes([bytes[2], bytes[3]]);

        let mut options = Vec::new();
        if bytes.len() > 4 {
            options
                .extend_from_slice(&bytes[4..])
                .map_err(|()| L2capError::PayloadTooLarge)?;
        }

        Ok(Self::new(destination_cid, flags, options))
    }

    /// Convert to bytes
    ///
    /// # Errors
    /// Returns `L2capError::PayloadTooLarge` if result buffer is too small
    pub fn to_bytes(&self) -> Result<Vec<u8, N>, L2capError> {
        let mut bytes = Vec::new();

        bytes
            .extend_from_slice(&self.destination_cid.to_le_bytes())
            .map_err(|()| L2capError::PayloadTooLarge)?;
        bytes
            .extend_from_slice(&self.flags.to_le_bytes())
            .map_err(|()| L2capError::PayloadTooLarge)?;
        bytes
            .extend_from_slice(&self.options)
            .map_err(|()| L2capError::PayloadTooLarge)?;

        Ok(bytes)
    }

    /// Get minimum size (header only)
    pub const MIN_SIZE: usize = 4;
}

/// L2CAP Configuration Response
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigurationResponse<const N: usize = 64> {
    /// Source Channel Identifier
    pub source_cid: ChannelId,
    /// Configuration flags
    pub flags: u16,
    /// Configuration result
    pub result: ConfigurationResult,
    /// Configuration options
    pub options: Vec<u8, N>,
}

impl<const N: usize> ConfigurationResponse<N> {
    /// Create new configuration response
    #[must_use]
    pub fn new(
        source_cid: ChannelId,
        flags: u16,
        result: ConfigurationResult,
        options: Vec<u8, N>,
    ) -> Self {
        Self {
            source_cid,
            flags,
            result,
            options,
        }
    }

    /// Create success response
    #[must_use]
    pub fn success(source_cid: ChannelId, flags: u16) -> Self {
        Self::new(source_cid, flags, ConfigurationResult::Success, Vec::new())
    }

    /// Parse from bytes
    ///
    /// # Errors
    /// Returns `L2capError` if not enough bytes, invalid result, or payload too large
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, L2capError> {
        if bytes.len() < 6 {
            return Err(L2capError::InsufficientData);
        }

        let source_cid = u16::from_le_bytes([bytes[0], bytes[1]]);
        let flags = u16::from_le_bytes([bytes[2], bytes[3]]);
        let result_code = u16::from_le_bytes([bytes[4], bytes[5]]);

        let result =
            ConfigurationResult::from_u16(result_code).ok_or(L2capError::InsufficientData)?;

        let mut options = Vec::new();
        if bytes.len() > 6 {
            options
                .extend_from_slice(&bytes[6..])
                .map_err(|()| L2capError::PayloadTooLarge)?;
        }

        Ok(Self::new(source_cid, flags, result, options))
    }

    /// Convert to bytes
    ///
    /// # Errors
    /// Returns `L2capError::PayloadTooLarge` if result buffer is too small
    pub fn to_bytes(&self) -> Result<Vec<u8, N>, L2capError> {
        let mut bytes = Vec::new();

        bytes
            .extend_from_slice(&self.source_cid.to_le_bytes())
            .map_err(|()| L2capError::PayloadTooLarge)?;
        bytes
            .extend_from_slice(&self.flags.to_le_bytes())
            .map_err(|()| L2capError::PayloadTooLarge)?;
        bytes
            .extend_from_slice(&(self.result as u16).to_le_bytes())
            .map_err(|()| L2capError::PayloadTooLarge)?;
        bytes
            .extend_from_slice(&self.options)
            .map_err(|()| L2capError::PayloadTooLarge)?;

        Ok(bytes)
    }

    /// Get minimum size (header only)
    pub const MIN_SIZE: usize = 6;
}

/// L2CAP Disconnection Request
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisconnectionRequest {
    /// Destination Channel Identifier
    pub destination_cid: ChannelId,
    /// Source Channel Identifier
    pub source_cid: ChannelId,
}

impl DisconnectionRequest {
    /// Create new disconnection request
    #[must_use]
    pub fn new(destination_cid: ChannelId, source_cid: ChannelId) -> Self {
        Self {
            destination_cid,
            source_cid,
        }
    }

    /// Parse from bytes
    ///
    /// # Errors
    /// Returns `L2capError::InsufficientData` if not enough bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, L2capError> {
        if bytes.len() < 4 {
            return Err(L2capError::InsufficientData);
        }

        let destination_cid = u16::from_le_bytes([bytes[0], bytes[1]]);
        let source_cid = u16::from_le_bytes([bytes[2], bytes[3]]);

        Ok(Self::new(destination_cid, source_cid))
    }

    /// Convert to bytes
    #[must_use]
    pub fn to_bytes(self) -> [u8; 4] {
        let mut bytes = [0u8; 4];
        bytes[0..2].copy_from_slice(&self.destination_cid.to_le_bytes());
        bytes[2..4].copy_from_slice(&self.source_cid.to_le_bytes());
        bytes
    }

    /// Size in bytes
    pub const SIZE: usize = 4;
}

/// L2CAP Disconnection Response
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisconnectionResponse {
    /// Destination Channel Identifier
    pub destination_cid: ChannelId,
    /// Source Channel Identifier
    pub source_cid: ChannelId,
}

impl DisconnectionResponse {
    /// Create new disconnection response
    #[must_use]
    pub fn new(destination_cid: ChannelId, source_cid: ChannelId) -> Self {
        Self {
            destination_cid,
            source_cid,
        }
    }

    /// Parse from bytes
    ///
    /// # Errors
    /// Returns `L2capError::InsufficientData` if not enough bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, L2capError> {
        if bytes.len() < 4 {
            return Err(L2capError::InsufficientData);
        }

        let destination_cid = u16::from_le_bytes([bytes[0], bytes[1]]);
        let source_cid = u16::from_le_bytes([bytes[2], bytes[3]]);

        Ok(Self::new(destination_cid, source_cid))
    }

    /// Convert to bytes
    #[must_use]
    pub fn to_bytes(self) -> [u8; 4] {
        let mut bytes = [0u8; 4];
        bytes[0..2].copy_from_slice(&self.destination_cid.to_le_bytes());
        bytes[2..4].copy_from_slice(&self.source_cid.to_le_bytes());
        bytes
    }

    /// Size in bytes
    pub const SIZE: usize = 4;
}

/// L2CAP Signaling Command
///
/// Represents any L2CAP signaling command with its header and payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignalingCommand<const N: usize = 64> {
    /// Connection Request
    ConnectionRequest {
        /// Command header
        header: SignalingHeader,
        /// Connection request payload
        payload: ConnectionRequest,
    },
    /// Connection Response
    ConnectionResponse {
        /// Command header
        header: SignalingHeader,
        /// Connection response payload
        payload: ConnectionResponse,
    },
    /// Configuration Request
    ConfigurationRequest {
        /// Command header
        header: SignalingHeader,
        /// Configuration request payload
        payload: ConfigurationRequest<N>,
    },
    /// Configuration Response
    ConfigurationResponse {
        /// Command header
        header: SignalingHeader,
        /// Configuration response payload
        payload: ConfigurationResponse<N>,
    },
    /// Disconnection Request
    DisconnectionRequest {
        /// Command header
        header: SignalingHeader,
        /// Disconnection request payload
        payload: DisconnectionRequest,
    },
    /// Disconnection Response
    DisconnectionResponse {
        /// Command header
        header: SignalingHeader,
        /// Disconnection response payload
        payload: DisconnectionResponse,
    },
    /// Unknown command (for forward compatibility)
    Unknown {
        /// Command header
        header: SignalingHeader,
        /// Command data
        data: Vec<u8, N>,
    },
}

impl<const N: usize> SignalingCommand<N> {
    /// Get the command header
    #[must_use]
    pub fn header(&self) -> SignalingHeader {
        match self {
            Self::ConnectionRequest { header, .. }
            | Self::ConnectionResponse { header, .. }
            | Self::ConfigurationRequest { header, .. }
            | Self::ConfigurationResponse { header, .. }
            | Self::DisconnectionRequest { header, .. }
            | Self::DisconnectionResponse { header, .. }
            | Self::Unknown { header, .. } => *header,
        }
    }

    /// Parse signaling command from bytes
    ///
    /// # Errors
    /// Returns `L2capError` if parsing fails
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, L2capError> {
        let header = SignalingHeader::from_bytes(bytes)?;

        if bytes.len() < SignalingHeader::SIZE + header.length as usize {
            return Err(L2capError::InsufficientData);
        }

        let payload_bytes =
            &bytes[SignalingHeader::SIZE..SignalingHeader::SIZE + header.length as usize];

        match header.code {
            SignalingCode::ConnectionRequest => {
                let payload = ConnectionRequest::from_bytes(payload_bytes)?;
                Ok(Self::ConnectionRequest { header, payload })
            }
            SignalingCode::ConnectionResponse => {
                let payload = ConnectionResponse::from_bytes(payload_bytes)?;
                Ok(Self::ConnectionResponse { header, payload })
            }
            SignalingCode::ConfigurationRequest => {
                let payload = ConfigurationRequest::from_bytes(payload_bytes)?;
                Ok(Self::ConfigurationRequest { header, payload })
            }
            SignalingCode::ConfigurationResponse => {
                let payload = ConfigurationResponse::from_bytes(payload_bytes)?;
                Ok(Self::ConfigurationResponse { header, payload })
            }
            SignalingCode::DisconnectionRequest => {
                let payload = DisconnectionRequest::from_bytes(payload_bytes)?;
                Ok(Self::DisconnectionRequest { header, payload })
            }
            SignalingCode::DisconnectionResponse => {
                let payload = DisconnectionResponse::from_bytes(payload_bytes)?;
                Ok(Self::DisconnectionResponse { header, payload })
            }
            _ => {
                let mut data = Vec::new();
                data.extend_from_slice(payload_bytes)
                    .map_err(|()| L2capError::PayloadTooLarge)?;
                Ok(Self::Unknown { header, data })
            }
        }
    }

    /// Convert signaling command to bytes
    ///
    /// # Errors
    /// Returns `L2capError::PayloadTooLarge` if result buffer is too small
    pub fn to_bytes(&self) -> Result<Vec<u8, N>, L2capError> {
        let mut bytes = Vec::new();

        bytes
            .extend_from_slice(&self.header().to_bytes())
            .map_err(|()| L2capError::PayloadTooLarge)?;

        match self {
            Self::ConnectionRequest { payload, .. } => {
                bytes
                    .extend_from_slice(&payload.to_bytes())
                    .map_err(|()| L2capError::PayloadTooLarge)?;
            }
            Self::ConnectionResponse { payload, .. } => {
                bytes
                    .extend_from_slice(&payload.to_bytes())
                    .map_err(|()| L2capError::PayloadTooLarge)?;
            }
            Self::ConfigurationRequest { payload, .. } => {
                let payload_bytes = payload.to_bytes()?;
                bytes
                    .extend_from_slice(&payload_bytes)
                    .map_err(|()| L2capError::PayloadTooLarge)?;
            }
            Self::ConfigurationResponse { payload, .. } => {
                let payload_bytes = payload.to_bytes()?;
                bytes
                    .extend_from_slice(&payload_bytes)
                    .map_err(|()| L2capError::PayloadTooLarge)?;
            }
            Self::DisconnectionRequest { payload, .. } => {
                bytes
                    .extend_from_slice(&payload.to_bytes())
                    .map_err(|()| L2capError::PayloadTooLarge)?;
            }
            Self::DisconnectionResponse { payload, .. } => {
                bytes
                    .extend_from_slice(&payload.to_bytes())
                    .map_err(|()| L2capError::PayloadTooLarge)?;
            }
            Self::Unknown { data, .. } => {
                bytes
                    .extend_from_slice(data)
                    .map_err(|()| L2capError::PayloadTooLarge)?;
            }
        }

        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signaling_header() {
        let header = SignalingHeader::new(SignalingCode::ConnectionRequest, 0x42, 4);
        let bytes = header.to_bytes();
        assert_eq!(bytes, [0x02, 0x42, 0x04, 0x00]);

        let parsed = SignalingHeader::from_bytes(&bytes).unwrap();
        assert_eq!(parsed, header);
    }

    #[test]
    fn test_connection_request() {
        let req = ConnectionRequest::new(0x0019, 0x0040); // AVDTP PSM, CID 0x0040
        let bytes = req.to_bytes();
        assert_eq!(bytes, [0x19, 0x00, 0x40, 0x00]);

        let parsed = ConnectionRequest::from_bytes(&bytes).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn test_connection_response() {
        let resp = ConnectionResponse::success(0x0041, 0x0040);
        let bytes = resp.to_bytes();
        assert_eq!(bytes, [0x41, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00]);

        let parsed = ConnectionResponse::from_bytes(&bytes).unwrap();
        assert_eq!(parsed, resp);
    }

    #[test]
    fn test_disconnection_request() {
        let req = DisconnectionRequest::new(0x0041, 0x0040);
        let bytes = req.to_bytes();
        assert_eq!(bytes, [0x41, 0x00, 0x40, 0x00]);

        let parsed = DisconnectionRequest::from_bytes(&bytes).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn test_signaling_command_parsing() {
        // Test connection request command
        let header = SignalingHeader::new(SignalingCode::ConnectionRequest, 0x01, 4);
        let conn_req = ConnectionRequest::new(0x0019, 0x0040);

        let mut command_bytes = Vec::<u8, 64>::new();
        command_bytes.extend_from_slice(&header.to_bytes()).unwrap();
        command_bytes
            .extend_from_slice(&conn_req.to_bytes())
            .unwrap();

        let parsed = SignalingCommand::<64>::from_bytes(&command_bytes).unwrap();

        match parsed {
            SignalingCommand::ConnectionRequest {
                header: h,
                payload: p,
            } => {
                assert_eq!(h, header);
                assert_eq!(p, conn_req);
            }
            _ => panic!("Wrong command type parsed"),
        }
    }

    #[test]
    fn test_configuration_request() {
        let mut options = Vec::<u8, 64>::new();
        options
            .extend_from_slice(&[0x01, 0x02, 0x03, 0x04])
            .unwrap();

        let req = ConfigurationRequest::new(0x0040, 0x0000, options.clone());
        let bytes = req.to_bytes().unwrap();

        let parsed: ConfigurationRequest<64> = ConfigurationRequest::from_bytes(&bytes).unwrap();
        assert_eq!(parsed.destination_cid, 0x0040);
        assert_eq!(parsed.flags, 0x0000);
        assert_eq!(parsed.options, options);
    }

    #[test]
    fn test_configuration_response() {
        let resp = ConfigurationResponse::<64>::success(0x0040, 0x0000);
        let bytes = resp.to_bytes().unwrap();

        let parsed: ConfigurationResponse<64> = ConfigurationResponse::from_bytes(&bytes).unwrap();
        assert_eq!(parsed.source_cid, 0x0040);
        assert_eq!(parsed.flags, 0x0000);
        assert_eq!(parsed.result, ConfigurationResult::Success);
        assert!(parsed.options.is_empty());
    }
}
