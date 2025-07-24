//! A2DP Profile Implementation
//!
//! This module provides the high-level A2DP profile implementation that manages
//! audio streaming sessions between devices using AVDTP over L2CAP.

use super::{
    A2DP_PSM, A2dpError, ConnectionState, Role, StreamEndpointId, StreamHandle,
    avdtp::{AvdtpProtocol, MediaType, StreamEndpoint},
    codec::{CodecCapabilities, SbcCapabilities},
};
use crate::{BluetoothAddress, l2cap::packet::ChannelId};
use heapless::{FnvIndexMap, Vec};

/// Maximum number of A2DP connections
pub const MAX_A2DP_CONNECTIONS: usize = 4;

/// A2DP Connection Information
#[derive(Debug, Clone)]
pub struct A2dpConnection {
    /// Remote device address
    pub remote_addr: BluetoothAddress,
    /// L2CAP channel ID for signaling
    pub signaling_cid: ChannelId,
    /// ACL connection handle
    pub conn_handle: u16,
    /// Connection state
    pub state: ConnectionState,
    /// Active streams for this connection
    pub streams: Vec<StreamHandle, 4>,
}

/// A2DP Profile Manager
///
/// Manages A2DP connections and audio streaming sessions. Integrates with
/// L2CAP for transport and AVDTP for stream management.
#[derive(Debug)]
pub struct A2dpProfile {
    /// AVDTP protocol handler
    avdtp: AvdtpProtocol,
    /// Active A2DP connections
    connections: FnvIndexMap<BluetoothAddress, A2dpConnection, MAX_A2DP_CONNECTIONS>,
    /// Local device role
    role: Role,
    /// Next SEID to assign
    next_seid: StreamEndpointId,
}

/// A2DP Stream Configuration
#[derive(Debug, Clone)]
pub struct StreamConfig {
    /// Codec configuration
    pub codec: CodecCapabilities,
    /// Maximum transmission unit for media packets
    pub mtu: u16,
    /// Stream priority
    pub priority: StreamPriority,
}

/// Stream Priority Levels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamPriority {
    /// Low priority stream
    Low,
    /// Normal priority stream  
    Normal,
    /// High priority stream
    High,
}

impl A2dpConnection {
    /// Create new A2DP connection
    #[must_use]
    pub fn new(remote_addr: BluetoothAddress, signaling_cid: ChannelId, conn_handle: u16) -> Self {
        Self {
            remote_addr,
            signaling_cid,
            conn_handle,
            state: ConnectionState::Connected,
            streams: Vec::new(),
        }
    }

    /// Add stream to connection
    ///
    /// # Errors
    /// Returns error if too many streams are already active
    pub fn add_stream(&mut self, handle: StreamHandle) -> Result<(), A2dpError> {
        self.streams
            .push(handle)
            .map_err(|_| A2dpError::ConfigurationFailed)
    }

    /// Remove stream from connection
    pub fn remove_stream(&mut self, handle: StreamHandle) {
        if let Some(pos) = self.streams.iter().position(|&h| h == handle) {
            self.streams.remove(pos);
        }
    }

    /// Check if connection has active streams
    #[must_use]
    pub fn has_active_streams(&self) -> bool {
        !self.streams.is_empty()
    }
}

impl A2dpProfile {
    /// Create new A2DP profile manager
    #[must_use]
    pub fn new(role: Role) -> Self {
        Self {
            avdtp: AvdtpProtocol::new(role),
            connections: FnvIndexMap::new(),
            role,
            next_seid: 1,
        }
    }

    /// Add SBC codec endpoint
    ///
    /// # Errors
    /// Returns error if endpoint cannot be added
    pub fn add_sbc_endpoint(
        &mut self,
        capabilities: SbcCapabilities,
    ) -> Result<StreamEndpointId, A2dpError> {
        let seid = self.next_seid;
        self.next_seid = self.next_seid.wrapping_add(1);

        let mut endpoint = StreamEndpoint::new(seid, self.role, MediaType::Audio);
        endpoint.add_capability(CodecCapabilities::Sbc(capabilities))?;

        self.avdtp.add_endpoint(endpoint)?;
        Ok(seid)
    }

    /// Create A2DP connection over existing L2CAP channel
    ///
    /// # Errors
    /// Returns error if connection cannot be established
    pub fn create_connection(
        &mut self,
        remote_addr: BluetoothAddress,
        signaling_cid: ChannelId,
        conn_handle: u16,
    ) -> Result<(), A2dpError> {
        if self.connections.contains_key(&remote_addr) {
            return Err(A2dpError::ConfigurationFailed);
        }

        let connection = A2dpConnection::new(remote_addr, signaling_cid, conn_handle);
        self.connections
            .insert(remote_addr, connection)
            .map_err(|_| A2dpError::ConfigurationFailed)?;

        Ok(())
    }

    /// Remove A2DP connection
    pub fn remove_connection(&mut self, remote_addr: &BluetoothAddress) -> Option<A2dpConnection> {
        if let Some(connection) = self.connections.remove(remote_addr) {
            // Clean up any active streams
            for stream_handle in &connection.streams {
                self.avdtp.remove_stream(*stream_handle);
            }
            Some(connection)
        } else {
            None
        }
    }

    /// Create audio stream
    ///
    /// # Errors
    /// Returns error if stream cannot be created
    pub fn create_stream(
        &mut self,
        remote_addr: BluetoothAddress,
        local_seid: StreamEndpointId,
        remote_seid: StreamEndpointId,
        config: StreamConfig,
    ) -> Result<StreamHandle, A2dpError> {
        // Verify connection exists
        let connection = self
            .connections
            .get_mut(&remote_addr)
            .ok_or(A2dpError::L2capError)?;

        // Configure local endpoint
        if let Some(endpoint) = self.avdtp.get_endpoint_mut(local_seid) {
            endpoint.configure(config.codec.clone())?;
        } else {
            return Err(A2dpError::InvalidEndpoint);
        }

        // Create AVDTP stream
        let stream_handle = self.avdtp.create_stream(
            local_seid,
            remote_seid,
            remote_addr,
            connection.conn_handle,
        )?;

        // Configure stream
        if let Some(stream) = self.avdtp.get_stream_mut(stream_handle) {
            stream.set_codec_config(config.codec);
        }

        // Add to connection
        connection.add_stream(stream_handle)?;

        Ok(stream_handle)
    }

    /// Start audio streaming
    ///
    /// # Errors
    /// Returns error if stream cannot be started
    pub fn start_stream(&mut self, handle: StreamHandle) -> Result<(), A2dpError> {
        if let Some(stream) = self.avdtp.get_stream_mut(handle) {
            stream.start()
        } else {
            Err(A2dpError::InvalidEndpoint)
        }
    }

    /// Suspend audio streaming
    ///
    /// # Errors
    /// Returns error if stream cannot be suspended
    pub fn suspend_stream(&mut self, handle: StreamHandle) -> Result<(), A2dpError> {
        if let Some(stream) = self.avdtp.get_stream_mut(handle) {
            stream.suspend()
        } else {
            Err(A2dpError::InvalidEndpoint)
        }
    }

    /// Close audio stream
    ///
    /// # Errors
    /// Returns error if stream is invalid
    pub fn close_stream(&mut self, handle: StreamHandle) -> Result<(), A2dpError> {
        if let Some(stream) = self.avdtp.remove_stream(handle) {
            // Remove from connection
            if let Some(connection) = self.connections.get_mut(&stream.remote_addr) {
                connection.remove_stream(handle);
            }

            // Reset endpoint
            if let Some(endpoint) = self.avdtp.get_endpoint_mut(stream.local_seid) {
                endpoint.reset();
            }

            Ok(())
        } else {
            Err(A2dpError::InvalidEndpoint)
        }
    }

    /// Get A2DP connection
    #[must_use]
    pub fn get_connection(&self, remote_addr: &BluetoothAddress) -> Option<&A2dpConnection> {
        self.connections.get(remote_addr)
    }

    /// Get stream information
    #[must_use]
    pub fn get_stream(&self, handle: StreamHandle) -> Option<&super::avdtp::AvdtpStream> {
        self.avdtp.get_stream(handle)
    }

    /// Get all available endpoints for discovery
    #[must_use]
    pub fn get_available_endpoints(&self) -> Vec<&StreamEndpoint, 4> {
        self.avdtp.get_available_endpoints()
    }

    /// Get device role
    #[must_use]
    pub const fn role(&self) -> Role {
        self.role
    }

    /// Get number of active connections
    #[must_use]
    pub fn connection_count(&self) -> usize {
        self.connections.len()
    }

    /// Check if device has active audio streams
    #[must_use]
    pub fn has_active_streams(&self) -> bool {
        self.connections
            .values()
            .any(A2dpConnection::has_active_streams)
    }

    /// Get PSM used by A2DP
    #[must_use]
    pub const fn psm() -> u16 {
        A2DP_PSM
    }
}

impl StreamConfig {
    /// Create stream configuration with SBC codec
    #[must_use]
    pub fn sbc(capabilities: SbcCapabilities) -> Self {
        Self {
            codec: CodecCapabilities::Sbc(capabilities),
            mtu: 672, // Standard A2DP MTU
            priority: StreamPriority::Normal,
        }
    }

    /// Create high quality SBC stream configuration
    #[must_use]
    pub fn sbc_high_quality() -> Self {
        Self::sbc(SbcCapabilities::high_quality())
    }

    /// Set MTU for stream
    #[must_use]
    pub fn with_mtu(mut self, mtu: u16) -> Self {
        self.mtu = mtu;
        self
    }

    /// Set priority for stream
    #[must_use]
    pub fn with_priority(mut self, priority: StreamPriority) -> Self {
        self.priority = priority;
        self
    }
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self::sbc_high_quality()
    }
}

impl Default for A2dpProfile {
    fn default() -> Self {
        Self::new(Role::Source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_a2dp_profile_creation() {
        let mut profile = A2dpProfile::new(Role::Source);
        assert_eq!(profile.role(), Role::Source);
        assert_eq!(profile.connection_count(), 0);
        assert!(!profile.has_active_streams());

        let sbc_caps = SbcCapabilities::default();
        let seid = profile.add_sbc_endpoint(sbc_caps).unwrap();
        assert_eq!(seid, 1);

        let endpoints = profile.get_available_endpoints();
        assert_eq!(endpoints.len(), 1);
    }

    #[test]
    fn test_a2dp_connection_management() {
        let mut profile = A2dpProfile::new(Role::Source);
        let remote_addr = BluetoothAddress::new([0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);

        profile.create_connection(remote_addr, 100, 200).unwrap();
        assert_eq!(profile.connection_count(), 1);

        let connection = profile.get_connection(&remote_addr).unwrap();
        assert_eq!(connection.remote_addr, remote_addr);
        assert_eq!(connection.signaling_cid, 100);
        assert_eq!(connection.conn_handle, 200);

        let removed = profile.remove_connection(&remote_addr);
        assert!(removed.is_some());
        assert_eq!(profile.connection_count(), 0);
    }

    #[test]
    fn test_stream_lifecycle() {
        let mut profile = A2dpProfile::new(Role::Source);
        let remote_addr = BluetoothAddress::new([0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);

        // Add endpoint and create connection
        let seid = profile
            .add_sbc_endpoint(SbcCapabilities::default())
            .unwrap();
        profile.create_connection(remote_addr, 100, 200).unwrap();

        // Create stream
        let config = StreamConfig::sbc_high_quality();
        let handle = profile
            .create_stream(remote_addr, seid, 10, config)
            .unwrap();

        let stream = profile.get_stream(handle).unwrap();
        assert_eq!(stream.state, ConnectionState::Configured);

        // Start stream
        profile.start_stream(handle).unwrap();
        let stream = profile.get_stream(handle).unwrap();
        assert_eq!(stream.state, ConnectionState::Streaming);

        // Suspend stream
        profile.suspend_stream(handle).unwrap();
        let stream = profile.get_stream(handle).unwrap();
        assert_eq!(stream.state, ConnectionState::Suspended);

        // Close stream
        profile.close_stream(handle).unwrap();
        assert!(profile.get_stream(handle).is_none());
    }

    #[test]
    fn test_stream_config() {
        let config = StreamConfig::sbc_high_quality();
        assert_eq!(config.mtu, 672);
        assert_eq!(config.priority, StreamPriority::Normal);

        let custom_config = StreamConfig::sbc_high_quality()
            .with_mtu(1024)
            .with_priority(StreamPriority::High);
        assert_eq!(custom_config.mtu, 1024);
        assert_eq!(custom_config.priority, StreamPriority::High);
    }
}
