//! AVDTP (Audio/Video Distribution Transport Protocol) Implementation
//!
//! AVDTP provides the transport protocol for multimedia streams over L2CAP.
//! It handles stream endpoint management, configuration negotiation, and
//! media packet transport.

use super::codec::{CodecCapabilities, CodecType};
use super::{A2dpError, ConnectionState, Role, StreamEndpointId, StreamHandle};
use crate::BluetoothAddress;
use heapless::{FnvIndexMap, Vec};

/// Maximum number of stream endpoints per device
pub const MAX_STREAM_ENDPOINTS: usize = 4;

/// Maximum number of active streams
pub const MAX_ACTIVE_STREAMS: usize = 2;

/// AVDTP Message Types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MessageType {
    /// Command message
    Command = 0x00,
    /// Response Accept
    ResponseAccept = 0x02,
    /// Response Reject
    ResponseReject = 0x03,
}

/// AVDTP Signal Identifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SignalId {
    /// Discover available stream endpoints
    Discover = 0x01,
    /// Get capabilities of a stream endpoint
    GetCapabilities = 0x02,
    /// Set configuration for a stream endpoint
    SetConfiguration = 0x03,
    /// Get current configuration
    GetConfiguration = 0x04,
    /// Reconfigure stream endpoint
    Reconfigure = 0x05,
    /// Open stream
    Open = 0x06,
    /// Start streaming
    Start = 0x07,
    /// Close stream
    Close = 0x08,
    /// Suspend stream
    Suspend = 0x09,
    /// Abort stream
    Abort = 0x0A,
}

/// AVDTP Stream Endpoint Information
#[derive(Debug, Clone)]
pub struct StreamEndpoint {
    /// Stream Endpoint Identifier
    pub seid: StreamEndpointId,
    /// Endpoint role (source or sink)
    pub role: Role,
    /// Media type (audio/video)
    pub media_type: MediaType,
    /// Whether endpoint is in use
    pub in_use: bool,
    /// Supported codec capabilities
    pub capabilities: Vec<CodecCapabilities, 4>,
    /// Current configuration (if configured)
    pub configuration: Option<CodecCapabilities>,
}

/// Media Types supported by AVDTP
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MediaType {
    /// Audio media
    Audio = 0x00,
    /// Video media  
    Video = 0x01,
    /// Multimedia media
    Multimedia = 0x02,
}

/// AVDTP Stream Information
#[derive(Debug, Clone)]
pub struct AvdtpStream {
    /// Stream handle
    pub handle: StreamHandle,
    /// Local stream endpoint ID
    pub local_seid: StreamEndpointId,
    /// Remote stream endpoint ID
    pub remote_seid: StreamEndpointId,
    /// Remote device address
    pub remote_addr: BluetoothAddress,
    /// Connection handle
    pub conn_handle: u16,
    /// Current stream state
    pub state: ConnectionState,
    /// Negotiated codec configuration
    pub codec_config: Option<CodecCapabilities>,
}

/// AVDTP Protocol Handler
#[derive(Debug)]
pub struct AvdtpProtocol {
    /// Local stream endpoints
    endpoints: Vec<StreamEndpoint, MAX_STREAM_ENDPOINTS>,
    /// Active streams
    streams: FnvIndexMap<StreamHandle, AvdtpStream, MAX_ACTIVE_STREAMS>,
    /// Next stream handle
    next_handle: StreamHandle,
    /// Local device role
    role: Role,
}

impl StreamEndpoint {
    /// Create a new stream endpoint
    #[must_use]
    pub fn new(seid: StreamEndpointId, role: Role, media_type: MediaType) -> Self {
        Self {
            seid,
            role,
            media_type,
            in_use: false,
            capabilities: Vec::new(),
            configuration: None,
        }
    }

    /// Add codec capability to endpoint
    ///
    /// # Errors
    /// Returns error if too many capabilities are added
    pub fn add_capability(&mut self, capability: CodecCapabilities) -> Result<(), A2dpError> {
        self.capabilities
            .push(capability)
            .map_err(|_| A2dpError::ConfigurationFailed)
    }

    /// Check if endpoint supports codec type
    #[must_use]
    pub fn supports_codec(&self, codec_type: CodecType) -> bool {
        self.capabilities
            .iter()
            .any(|cap| cap.codec_type() == codec_type)
    }

    /// Configure endpoint with codec
    ///
    /// # Errors
    /// Returns error if codec is not supported
    pub fn configure(&mut self, config: CodecCapabilities) -> Result<(), A2dpError> {
        if !self.supports_codec(config.codec_type()) {
            return Err(A2dpError::UnsupportedCodec);
        }

        self.configuration = Some(config);
        self.in_use = true;
        Ok(())
    }

    /// Reset endpoint configuration
    pub fn reset(&mut self) {
        self.configuration = None;
        self.in_use = false;
    }
}

impl AvdtpStream {
    /// Create a new AVDTP stream
    #[must_use]
    pub fn new(
        handle: StreamHandle,
        local_seid: StreamEndpointId,
        remote_seid: StreamEndpointId,
        remote_addr: BluetoothAddress,
        conn_handle: u16,
    ) -> Self {
        Self {
            handle,
            local_seid,
            remote_seid,
            remote_addr,
            conn_handle,
            state: ConnectionState::Connected,
            codec_config: None,
        }
    }

    /// Set codec configuration for stream
    pub fn set_codec_config(&mut self, config: CodecCapabilities) {
        self.codec_config = Some(config);
        self.state = ConnectionState::Configured;
    }

    /// Start streaming
    ///
    /// # Errors
    /// Returns error if stream is not ready for streaming
    pub fn start(&mut self) -> Result<(), A2dpError> {
        if self.state != ConnectionState::Configured && self.state != ConnectionState::Suspended {
            return Err(A2dpError::StreamNotReady);
        }

        self.state = ConnectionState::Streaming;
        Ok(())
    }

    /// Suspend streaming
    ///
    /// # Errors
    /// Returns error if stream is not currently streaming
    pub fn suspend(&mut self) -> Result<(), A2dpError> {
        if self.state != ConnectionState::Streaming {
            return Err(A2dpError::StreamNotReady);
        }

        self.state = ConnectionState::Suspended;
        Ok(())
    }

    /// Close stream
    pub fn close(&mut self) {
        self.state = ConnectionState::Connected;
        self.codec_config = None;
    }
}

impl AvdtpProtocol {
    /// Create new AVDTP protocol handler
    #[must_use]
    pub fn new(role: Role) -> Self {
        Self {
            endpoints: Vec::new(),
            streams: FnvIndexMap::new(),
            next_handle: 1,
            role,
        }
    }

    /// Add stream endpoint
    ///
    /// # Errors
    /// Returns error if too many endpoints are added
    pub fn add_endpoint(&mut self, endpoint: StreamEndpoint) -> Result<(), A2dpError> {
        self.endpoints
            .push(endpoint)
            .map_err(|_| A2dpError::ConfigurationFailed)
    }

    /// Get stream endpoint by SEID
    #[must_use]
    pub fn get_endpoint(&self, seid: StreamEndpointId) -> Option<&StreamEndpoint> {
        self.endpoints.iter().find(|ep| ep.seid == seid)
    }

    /// Get mutable stream endpoint by SEID
    pub fn get_endpoint_mut(&mut self, seid: StreamEndpointId) -> Option<&mut StreamEndpoint> {
        self.endpoints.iter_mut().find(|ep| ep.seid == seid)
    }

    /// Create new stream
    ///
    /// # Errors
    /// Returns error if endpoint is invalid or too many streams
    pub fn create_stream(
        &mut self,
        local_seid: StreamEndpointId,
        remote_seid: StreamEndpointId,
        remote_addr: BluetoothAddress,
        conn_handle: u16,
    ) -> Result<StreamHandle, A2dpError> {
        // Validate local endpoint exists
        if self.get_endpoint(local_seid).is_none() {
            return Err(A2dpError::InvalidEndpoint);
        }

        let handle = self.next_handle;
        self.next_handle = self.next_handle.wrapping_add(1);

        let stream = AvdtpStream::new(handle, local_seid, remote_seid, remote_addr, conn_handle);

        self.streams
            .insert(handle, stream)
            .map_err(|_| A2dpError::ConfigurationFailed)?;

        Ok(handle)
    }

    /// Get stream by handle
    #[must_use]
    pub fn get_stream(&self, handle: StreamHandle) -> Option<&AvdtpStream> {
        self.streams.get(&handle)
    }

    /// Get mutable stream by handle
    pub fn get_stream_mut(&mut self, handle: StreamHandle) -> Option<&mut AvdtpStream> {
        self.streams.get_mut(&handle)
    }

    /// Remove stream
    pub fn remove_stream(&mut self, handle: StreamHandle) -> Option<AvdtpStream> {
        self.streams.remove(&handle)
    }

    /// Get all available endpoints for discovery
    #[must_use]
    pub fn get_available_endpoints(&self) -> Vec<&StreamEndpoint, MAX_STREAM_ENDPOINTS> {
        self.endpoints.iter().filter(|ep| !ep.in_use).collect()
    }

    /// Get device role
    #[must_use]
    pub const fn role(&self) -> Role {
        self.role
    }
}

impl Default for AvdtpProtocol {
    fn default() -> Self {
        Self::new(Role::Source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::a2dp::codec::{CodecCapabilities, SbcCapabilities};

    #[test]
    fn test_stream_endpoint_creation() {
        let mut endpoint = StreamEndpoint::new(1, Role::Source, MediaType::Audio);
        assert_eq!(endpoint.seid, 1);
        assert_eq!(endpoint.role, Role::Source);
        assert_eq!(endpoint.media_type, MediaType::Audio);
        assert!(!endpoint.in_use);

        let sbc_caps = CodecCapabilities::Sbc(SbcCapabilities::default());
        endpoint.add_capability(sbc_caps.clone()).unwrap();
        assert!(endpoint.supports_codec(CodecType::Sbc));

        endpoint.configure(sbc_caps).unwrap();
        assert!(endpoint.in_use);
        assert!(endpoint.configuration.is_some());
    }

    #[test]
    fn test_avdtp_protocol() {
        let mut protocol = AvdtpProtocol::new(Role::Source);
        assert_eq!(protocol.role(), Role::Source);

        let endpoint = StreamEndpoint::new(1, Role::Source, MediaType::Audio);
        protocol.add_endpoint(endpoint).unwrap();

        assert!(protocol.get_endpoint(1).is_some());
        assert!(protocol.get_endpoint(2).is_none());

        let available = protocol.get_available_endpoints();
        assert_eq!(available.len(), 1);
    }

    #[test]
    fn test_stream_lifecycle() {
        let mut protocol = AvdtpProtocol::new(Role::Source);
        let endpoint = StreamEndpoint::new(1, Role::Source, MediaType::Audio);
        protocol.add_endpoint(endpoint).unwrap();

        let addr = crate::BluetoothAddress::new([0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);
        let handle = protocol.create_stream(1, 2, addr, 100).unwrap();

        assert!(protocol.get_stream(handle).is_some());

        let stream = protocol.get_stream_mut(handle).unwrap();
        assert_eq!(stream.state, ConnectionState::Connected);

        let sbc_config = CodecCapabilities::Sbc(SbcCapabilities::default());
        stream.set_codec_config(sbc_config);
        assert_eq!(stream.state, ConnectionState::Configured);

        stream.start().unwrap();
        assert_eq!(stream.state, ConnectionState::Streaming);

        stream.suspend().unwrap();
        assert_eq!(stream.state, ConnectionState::Suspended);

        stream.close();
        assert_eq!(stream.state, ConnectionState::Connected);
    }
}
