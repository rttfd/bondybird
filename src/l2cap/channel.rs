//! L2CAP Channel Management
//!
//! This module implements L2CAP connection-oriented channels with state management,
//! configuration parameters, and flow control as defined in the Bluetooth Core Specification.

use super::packet::{ChannelId, L2capError, ProtocolServiceMultiplexer, cid};
use crate::BluetoothAddress;
use heapless::FnvIndexMap;

/// Maximum number of L2CAP channels per connection
pub const MAX_L2CAP_CHANNELS: usize = 8;

/// L2CAP Channel State
///
/// Represents the current state of an L2CAP connection-oriented channel
/// according to the Bluetooth Core Specification state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelState {
    /// Channel is closed - no connection exists
    Closed,
    /// Waiting for Connect Response after sending Connect Request
    WaitConnect,
    /// Waiting for Connect Response after receiving Connect Request
    WaitConnectRsp,
    /// Channel connection established, configuration in progress
    Config,
    /// Channel is open and ready for data transfer
    Open,
    /// Waiting for Disconnect Response after sending Disconnect Request
    WaitDisconnect,
}

impl Default for ChannelState {
    fn default() -> Self {
        Self::Closed
    }
}

/// L2CAP Configuration Parameters
///
/// Parameters negotiated during channel configuration phase.
/// These control data flow, packet sizes, and quality of service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigurationOptions {
    /// Maximum Transmission Unit - largest packet size that can be sent
    pub mtu: u16,
    /// Flush timeout in milliseconds (0 = no flush timeout)
    pub flush_timeout: u16,
    /// Quality of Service parameters
    pub qos: Option<QualityOfService>,
    /// Flow control mode
    pub flow_control_mode: FlowControlMode,
}

impl Default for ConfigurationOptions {
    fn default() -> Self {
        Self {
            mtu: 672, // Default L2CAP MTU
            flush_timeout: 0,
            qos: None,
            flow_control_mode: FlowControlMode::BasicMode,
        }
    }
}

/// Quality of Service Parameters
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QualityOfService {
    /// Service type
    pub service_type: QosServiceType,
    /// Token rate in octets per second
    pub token_rate: u32,
    /// Token bucket size in octets
    pub token_bucket_size: u32,
    /// Peak bandwidth in octets per second
    pub peak_bandwidth: u32,
    /// Latency in microseconds
    pub latency: u32,
    /// Delay variation in microseconds
    pub delay_variation: u32,
}

/// `QoS` Service Types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum QosServiceType {
    /// No traffic (channel disabled)
    NoTraffic = 0x00,
    /// Best effort service
    BestEffort = 0x01,
    /// Guaranteed service
    Guaranteed = 0x02,
}

/// Flow Control Modes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FlowControlMode {
    /// Basic L2CAP mode (no flow control)
    BasicMode = 0x00,
    /// Retransmission mode
    RetransmissionMode = 0x01,
    /// Flow control mode
    FlowControlMode = 0x02,
    /// Enhanced retransmission mode
    EnhancedRetransmissionMode = 0x03,
    /// Streaming mode
    StreamingMode = 0x04,
}

/// L2CAP Channel Information
///
/// Represents a connection-oriented L2CAP channel with its current state,
/// configuration, and connection details.
#[derive(Debug, Clone)]
pub struct L2capChannel {
    /// Local channel identifier
    pub local_cid: ChannelId,
    /// Remote channel identifier (valid after connection established)
    pub remote_cid: Option<ChannelId>,
    /// Protocol Service Multiplexer
    pub psm: ProtocolServiceMultiplexer,
    /// Remote device address
    pub remote_addr: BluetoothAddress,
    /// ACL connection handle
    pub conn_handle: u16,
    /// Current channel state
    pub state: ChannelState,
    /// Local configuration parameters
    pub local_config: ConfigurationOptions,
    /// Remote configuration parameters (negotiated)
    pub remote_config: Option<ConfigurationOptions>,
    /// Configuration flags tracking what has been configured
    pub config_flags: ConfigurationFlags,
}

impl L2capChannel {
    /// Create a new L2CAP channel for outgoing connection
    #[must_use]
    pub fn new_outgoing(
        local_cid: ChannelId,
        psm: ProtocolServiceMultiplexer,
        remote_addr: BluetoothAddress,
        conn_handle: u16,
    ) -> Self {
        Self {
            local_cid,
            remote_cid: None,
            psm,
            remote_addr,
            conn_handle,
            state: ChannelState::Closed,
            local_config: ConfigurationOptions::default(),
            remote_config: None,
            config_flags: ConfigurationFlags::default(),
        }
    }

    /// Create a new L2CAP channel for incoming connection
    #[must_use]
    pub fn new_incoming(
        local_cid: ChannelId,
        remote_cid: ChannelId,
        psm: ProtocolServiceMultiplexer,
        remote_addr: BluetoothAddress,
        conn_handle: u16,
    ) -> Self {
        Self {
            local_cid,
            remote_cid: Some(remote_cid),
            psm,
            remote_addr,
            conn_handle,
            state: ChannelState::WaitConnectRsp,
            local_config: ConfigurationOptions::default(),
            remote_config: None,
            config_flags: ConfigurationFlags::default(),
        }
    }

    /// Update channel state
    pub fn set_state(&mut self, state: ChannelState) {
        self.state = state;
    }

    /// Set remote channel identifier (when connection is established)
    pub fn set_remote_cid(&mut self, remote_cid: ChannelId) {
        self.remote_cid = Some(remote_cid);
    }

    /// Set remote configuration options
    pub fn set_remote_config(&mut self, config: ConfigurationOptions) {
        self.remote_config = Some(config);
    }

    /// Check if channel is ready for data transfer
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.state == ChannelState::Open
    }

    /// Check if channel configuration is complete
    #[must_use]
    pub fn is_configured(&self) -> bool {
        self.config_flags.local_state == ConfigurationState::Complete
            && self.config_flags.remote_state == ConfigurationState::Complete
    }

    /// Get effective MTU (minimum of local and remote MTU)
    #[must_use]
    pub fn effective_mtu(&self) -> u16 {
        if let Some(remote_config) = &self.remote_config {
            core::cmp::min(self.local_config.mtu, remote_config.mtu)
        } else {
            self.local_config.mtu
        }
    }
}

/// Configuration state for each side of the connection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConfigurationState {
    /// Not yet configured
    #[default]
    NotConfigured,
    /// Configuration in progress
    InProgress,
    /// Configuration complete
    Complete,
}

/// Configuration state tracking flags
#[derive(Debug, Clone, Copy, Default)]
pub struct ConfigurationFlags {
    /// Local side configuration state
    pub local_state: ConfigurationState,
    /// Remote side configuration state  
    pub remote_state: ConfigurationState,
}

/// L2CAP Channel Manager
///
/// Manages multiple L2CAP channels for a single ACL connection.
/// Handles channel creation, state transitions, and configuration.
#[derive(Debug)]
pub struct ChannelManager {
    /// Map of local CID to channel information
    channels: FnvIndexMap<ChannelId, L2capChannel, MAX_L2CAP_CHANNELS>,
    /// Next available local CID for dynamic allocation
    next_local_cid: ChannelId,
}

impl ChannelManager {
    /// Create a new channel manager
    #[must_use]
    pub fn new() -> Self {
        Self {
            channels: FnvIndexMap::new(),
            next_local_cid: cid::DYNAMIC_START,
        }
    }

    /// Allocate a new local channel identifier
    ///
    /// # Errors
    /// Returns `L2capError::PayloadTooLarge` if no more CIDs are available
    pub fn allocate_cid(&mut self) -> Result<ChannelId, L2capError> {
        // Search for next available CID
        for _ in 0..0xFFFF {
            let cid = self.next_local_cid;
            self.next_local_cid = self.next_local_cid.wrapping_add(1);

            // Skip reserved CIDs
            if self.next_local_cid < cid::DYNAMIC_START {
                self.next_local_cid = cid::DYNAMIC_START;
            }

            if !self.channels.contains_key(&cid) {
                return Ok(cid);
            }
        }

        Err(L2capError::PayloadTooLarge) // No available CIDs
    }

    /// Create a new outgoing L2CAP channel
    ///
    /// # Errors
    /// Returns `L2capError::PayloadTooLarge` if no more channels can be created
    pub fn create_outgoing_channel(
        &mut self,
        psm: ProtocolServiceMultiplexer,
        remote_addr: BluetoothAddress,
        conn_handle: u16,
    ) -> Result<ChannelId, L2capError> {
        let local_cid = self.allocate_cid()?;
        let channel = L2capChannel::new_outgoing(local_cid, psm, remote_addr, conn_handle);

        self.channels
            .insert(local_cid, channel)
            .map_err(|_| L2capError::PayloadTooLarge)?;

        Ok(local_cid)
    }

    /// Create a new incoming L2CAP channel
    ///
    /// # Errors
    /// Returns `L2capError::PayloadTooLarge` if no more channels can be created
    pub fn create_incoming_channel(
        &mut self,
        remote_cid: ChannelId,
        psm: ProtocolServiceMultiplexer,
        remote_addr: BluetoothAddress,
        conn_handle: u16,
    ) -> Result<ChannelId, L2capError> {
        let local_cid = self.allocate_cid()?;
        let channel =
            L2capChannel::new_incoming(local_cid, remote_cid, psm, remote_addr, conn_handle);

        self.channels
            .insert(local_cid, channel)
            .map_err(|_| L2capError::PayloadTooLarge)?;

        Ok(local_cid)
    }

    /// Get a channel by local CID
    #[must_use]
    pub fn get_channel(&self, local_cid: ChannelId) -> Option<&L2capChannel> {
        self.channels.get(&local_cid)
    }

    /// Get a mutable channel by local CID
    pub fn get_channel_mut(&mut self, local_cid: ChannelId) -> Option<&mut L2capChannel> {
        self.channels.get_mut(&local_cid)
    }

    /// Remove a channel
    pub fn remove_channel(&mut self, local_cid: ChannelId) -> Option<L2capChannel> {
        self.channels.remove(&local_cid)
    }

    /// Get all channels
    #[must_use]
    pub fn channels(&self) -> &FnvIndexMap<ChannelId, L2capChannel, MAX_L2CAP_CHANNELS> {
        &self.channels
    }

    /// Get number of active channels
    #[must_use]
    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }

    /// Find channel by remote CID and connection handle
    #[must_use]
    pub fn find_by_remote_cid(
        &self,
        remote_cid: ChannelId,
        conn_handle: u16,
    ) -> Option<&L2capChannel> {
        self.channels.values().find(|channel| {
            channel.remote_cid == Some(remote_cid) && channel.conn_handle == conn_handle
        })
    }

    /// Find channel by PSM and connection handle
    #[must_use]
    pub fn find_by_psm(
        &self,
        psm: ProtocolServiceMultiplexer,
        conn_handle: u16,
    ) -> Option<&L2capChannel> {
        self.channels
            .values()
            .find(|channel| channel.psm == psm && channel.conn_handle == conn_handle)
    }

    /// Get all open channels for a connection handle
    pub fn get_open_channels(&self, conn_handle: u16) -> impl Iterator<Item = &L2capChannel> {
        self.channels
            .values()
            .filter(move |channel| channel.conn_handle == conn_handle && channel.is_open())
    }
}

impl Default for ChannelManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::super::packet::psm;
    use super::*;
    use crate::BluetoothAddress;

    #[test]
    fn test_channel_state_transitions() {
        let mut channel = L2capChannel::new_outgoing(
            0x0040,
            psm::AVDTP,
            BluetoothAddress::new([0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC]),
            0x0001,
        );

        assert_eq!(channel.state, ChannelState::Closed);
        assert!(!channel.is_open());

        channel.set_state(ChannelState::WaitConnect);
        assert_eq!(channel.state, ChannelState::WaitConnect);

        channel.set_state(ChannelState::Open);
        assert_eq!(channel.state, ChannelState::Open);
        assert!(channel.is_open());
    }

    #[test]
    fn test_channel_configuration() {
        let mut channel = L2capChannel::new_outgoing(
            0x0040,
            psm::AVDTP,
            BluetoothAddress::new([0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC]),
            0x0001,
        );

        assert!(!channel.is_configured());

        // Set remote configuration
        let remote_config = ConfigurationOptions {
            mtu: 1024,
            flush_timeout: 0,
            qos: None,
            flow_control_mode: FlowControlMode::BasicMode,
        };
        channel.set_remote_config(remote_config.clone());

        // Mark as configured
        channel.config_flags.local_state = ConfigurationState::Complete;
        channel.config_flags.remote_state = ConfigurationState::Complete;

        assert!(channel.is_configured());
        assert_eq!(channel.effective_mtu(), 672); // Min of 672 (local) and 1024 (remote)
    }

    #[test]
    fn test_channel_manager() {
        let mut manager = ChannelManager::new();
        let remote_addr = BluetoothAddress::new([0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC]);

        // Create outgoing channel
        let local_cid = manager
            .create_outgoing_channel(psm::AVDTP, remote_addr, 0x0001)
            .unwrap();

        assert_eq!(manager.channel_count(), 1);
        assert!(local_cid >= cid::DYNAMIC_START);

        // Get channel
        let channel = manager.get_channel(local_cid).unwrap();
        assert_eq!(channel.local_cid, local_cid);
        assert_eq!(channel.psm, psm::AVDTP);
        assert_eq!(channel.remote_addr, remote_addr);

        // Remove channel
        let removed = manager.remove_channel(local_cid).unwrap();
        assert_eq!(removed.local_cid, local_cid);
        assert_eq!(manager.channel_count(), 0);
    }

    #[test]
    fn test_cid_allocation() {
        let mut manager = ChannelManager::new();

        // Allocate multiple CIDs
        let cid1 = manager.allocate_cid().unwrap();
        let cid2 = manager.allocate_cid().unwrap();
        let cid3 = manager.allocate_cid().unwrap();

        assert!(cid1 >= cid::DYNAMIC_START);
        assert!(cid2 >= cid::DYNAMIC_START);
        assert!(cid3 >= cid::DYNAMIC_START);

        // All should be different
        assert_ne!(cid1, cid2);
        assert_ne!(cid2, cid3);
        assert_ne!(cid1, cid3);
    }

    #[test]
    fn test_channel_lookup() {
        let mut manager = ChannelManager::new();
        let remote_addr = BluetoothAddress::new([0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC]);

        // Create incoming channel
        let local_cid = manager
            .create_incoming_channel(
                0x0050, // remote CID
                psm::AVDTP,
                remote_addr,
                0x0001,
            )
            .unwrap();

        // Find by remote CID
        let found = manager.find_by_remote_cid(0x0050, 0x0001).unwrap();
        assert_eq!(found.local_cid, local_cid);

        // Find by PSM
        let found = manager.find_by_psm(psm::AVDTP, 0x0001).unwrap();
        assert_eq!(found.local_cid, local_cid);
    }

    #[test]
    fn test_configuration_options_default() {
        let config = ConfigurationOptions::default();
        assert_eq!(config.mtu, 672);
        assert_eq!(config.flush_timeout, 0);
        assert_eq!(config.qos, None);
        assert_eq!(config.flow_control_mode, FlowControlMode::BasicMode);
    }
}
