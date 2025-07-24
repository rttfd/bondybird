//! ACL (Asynchronous Connection-Less) Data Integration
//!
//! This module implements ACL data packet handling and routing to L2CAP.
//! It manages ACL connections and routes L2CAP packets between the HCI layer
//! and the L2CAP signaling processor.

use crate::{
    BluetoothAddress,
    l2cap::{packet::L2capPacket, processor::SignalingProcessor},
};
use heapless::{FnvIndexMap, Vec};

/// Maximum number of ACL connections
pub const MAX_ACL_CONNECTIONS: usize = 4;

/// Maximum ACL data packet size
pub const MAX_ACL_DATA_SIZE: usize = 1024;

/// ACL Packet Boundary flags
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PacketBoundary {
    /// First non-flushable packet
    FirstNonFlushable = 0x00,
    /// Continuing fragment packet
    ContinuingFragment = 0x01,
    /// First flushable packet (deprecated)
    FirstFlushable = 0x02,
    /// Complete L2CAP PDU (no fragmentation)
    CompletePdu = 0x03,
}

impl PacketBoundary {
    /// Convert from raw 2-bit value
    #[must_use]
    pub fn from_u8(value: u8) -> Option<Self> {
        match value & 0x03 {
            0x00 => Some(Self::FirstNonFlushable),
            0x01 => Some(Self::ContinuingFragment),
            0x02 => Some(Self::FirstFlushable),
            0x03 => Some(Self::CompletePdu),
            _ => None,
        }
    }
}

/// ACL Broadcast flags
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BroadcastFlag {
    /// Point-to-point (no broadcast)
    PointToPoint = 0x00,
    /// Active broadcast
    ActiveBroadcast = 0x01,
    /// Piconet broadcast
    PiconetBroadcast = 0x02,
    /// Reserved
    Reserved = 0x03,
}

impl BroadcastFlag {
    /// Convert from raw 2-bit value
    #[must_use]
    pub fn from_u8(value: u8) -> Option<Self> {
        match value & 0x03 {
            0x00 => Some(Self::PointToPoint),
            0x01 => Some(Self::ActiveBroadcast),
            0x02 => Some(Self::PiconetBroadcast),
            0x03 => Some(Self::Reserved),
            _ => None,
        }
    }
}

/// ACL Data packet header
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AclHeader {
    /// Connection handle (12 bits)
    pub connection_handle: u16,
    /// Packet boundary flag (2 bits)
    pub packet_boundary: PacketBoundary,
    /// Broadcast flag (2 bits)
    pub broadcast_flag: BroadcastFlag,
    /// Data total length
    pub data_length: u16,
}

impl AclHeader {
    /// Create new ACL header
    #[must_use]
    pub fn new(
        connection_handle: u16,
        packet_boundary: PacketBoundary,
        broadcast_flag: BroadcastFlag,
        data_length: u16,
    ) -> Self {
        Self {
            connection_handle: connection_handle & 0x0FFF, // Only 12 bits
            packet_boundary,
            broadcast_flag,
            data_length,
        }
    }

    /// Parse ACL header from bytes
    ///
    /// # Errors
    /// Returns error if not enough bytes or invalid flags
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() < 4 {
            return Err("Insufficient data for ACL header");
        }

        let handle_and_flags = u16::from_le_bytes([bytes[0], bytes[1]]);
        let connection_handle = handle_and_flags & 0x0FFF;
        let pb_flag = (handle_and_flags >> 12) as u8 & 0x03;
        let bc_flag = (handle_and_flags >> 14) as u8 & 0x03;
        let data_length = u16::from_le_bytes([bytes[2], bytes[3]]);

        let packet_boundary =
            PacketBoundary::from_u8(pb_flag).ok_or("Invalid packet boundary flag")?;
        let broadcast_flag = BroadcastFlag::from_u8(bc_flag).ok_or("Invalid broadcast flag")?;

        Ok(Self::new(
            connection_handle,
            packet_boundary,
            broadcast_flag,
            data_length,
        ))
    }

    /// Convert header to bytes
    #[must_use]
    pub fn to_bytes(self) -> [u8; 4] {
        let handle_and_flags = self.connection_handle
            | ((self.packet_boundary as u16) << 12)
            | ((self.broadcast_flag as u16) << 14);

        let mut bytes = [0u8; 4];
        bytes[0..2].copy_from_slice(&handle_and_flags.to_le_bytes());
        bytes[2..4].copy_from_slice(&self.data_length.to_le_bytes());
        bytes
    }

    /// Size of ACL header in bytes
    pub const SIZE: usize = 4;
}

/// ACL Data packet
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AclPacket<const N: usize = MAX_ACL_DATA_SIZE> {
    /// ACL header
    pub header: AclHeader,
    /// Packet data
    pub data: Vec<u8, N>,
}

impl<const N: usize> AclPacket<N> {
    /// Create new ACL packet
    #[must_use]
    pub fn new(header: AclHeader, data: Vec<u8, N>) -> Self {
        Self { header, data }
    }

    /// Parse ACL packet from bytes
    ///
    /// # Errors
    /// Returns error if parsing fails or data too large
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, &'static str> {
        let header = AclHeader::from_bytes(bytes)?;

        if bytes.len() < AclHeader::SIZE + header.data_length as usize {
            return Err("Insufficient data for ACL packet");
        }

        let mut data = Vec::new();
        let data_bytes = &bytes[AclHeader::SIZE..AclHeader::SIZE + header.data_length as usize];
        data.extend_from_slice(data_bytes)
            .map_err(|()| "ACL data too large")?;

        Ok(Self::new(header, data))
    }

    /// Convert packet to bytes
    ///
    /// # Errors
    /// Returns error if result buffer too small
    pub fn to_bytes(&self) -> Result<Vec<u8, N>, &'static str> {
        let mut bytes = Vec::new();

        bytes
            .extend_from_slice(&self.header.to_bytes())
            .map_err(|()| "Buffer too small for ACL header")?;
        bytes
            .extend_from_slice(&self.data)
            .map_err(|()| "Buffer too small for ACL data")?;

        Ok(bytes)
    }

    /// Check if this is a complete L2CAP PDU
    #[must_use]
    pub fn is_complete_pdu(&self) -> bool {
        matches!(self.header.packet_boundary, PacketBoundary::CompletePdu)
    }

    /// Check if this is the first fragment of a fragmented packet
    #[must_use]
    pub fn is_first_fragment(&self) -> bool {
        matches!(
            self.header.packet_boundary,
            PacketBoundary::FirstNonFlushable | PacketBoundary::FirstFlushable
        )
    }

    /// Check if this is a continuing fragment
    #[must_use]
    pub fn is_continuing_fragment(&self) -> bool {
        matches!(
            self.header.packet_boundary,
            PacketBoundary::ContinuingFragment
        )
    }
}

/// ACL Connection information
#[derive(Debug, Clone)]
pub struct AclConnection {
    /// Connection handle
    pub handle: u16,
    /// Remote device address
    pub remote_addr: BluetoothAddress,
    /// Fragmentation buffer for incomplete packets
    pub fragment_buffer: Vec<u8, MAX_ACL_DATA_SIZE>,
    /// Expected total length for fragmented packet
    pub expected_length: Option<u16>,
}

impl AclConnection {
    /// Create new ACL connection
    #[must_use]
    pub fn new(handle: u16, remote_addr: BluetoothAddress) -> Self {
        Self {
            handle,
            remote_addr,
            fragment_buffer: Vec::new(),
            expected_length: None,
        }
    }

    /// Reset fragmentation state
    pub fn reset_fragmentation(&mut self) {
        self.fragment_buffer.clear();
        self.expected_length = None;
    }

    /// Add fragment to buffer
    ///
    /// # Errors
    /// Returns error if fragment cannot be added
    pub fn add_fragment(&mut self, data: &[u8]) -> Result<(), &'static str> {
        self.fragment_buffer
            .extend_from_slice(data)
            .map_err(|()| "Fragment buffer full")?;
        Ok(())
    }

    /// Check if fragmented packet is complete
    #[must_use]
    pub fn is_fragment_complete(&self) -> bool {
        if let Some(expected) = self.expected_length {
            self.fragment_buffer.len() >= expected as usize
        } else {
            false
        }
    }

    /// Get complete fragmented data
    #[must_use]
    pub fn get_complete_data(&self) -> Option<&[u8]> {
        if self.is_fragment_complete() {
            Some(&self.fragment_buffer)
        } else {
            None
        }
    }
}

/// ACL Data Manager
///
/// Manages ACL connections and handles packet fragmentation/reassembly.
/// Routes complete L2CAP packets to the signaling processor.
#[derive(Debug)]
pub struct AclManager {
    /// Map of connection handle to connection info
    connections: FnvIndexMap<u16, AclConnection, MAX_ACL_CONNECTIONS>,
    /// L2CAP signaling processor
    l2cap_processor: SignalingProcessor,
}

impl AclManager {
    /// Create new ACL manager
    #[must_use]
    pub fn new() -> Self {
        Self {
            connections: FnvIndexMap::new(),
            l2cap_processor: SignalingProcessor::new(),
        }
    }

    /// Add a new ACL connection
    ///
    /// # Errors
    /// Returns error if connection cannot be added
    pub fn add_connection(
        &mut self,
        handle: u16,
        remote_addr: BluetoothAddress,
    ) -> Result<(), &'static str> {
        let connection = AclConnection::new(handle, remote_addr);
        self.connections
            .insert(handle, connection)
            .map_err(|_| "Too many ACL connections")?;
        Ok(())
    }

    /// Remove ACL connection
    pub fn remove_connection(&mut self, handle: u16) -> Option<AclConnection> {
        self.connections.remove(&handle)
    }

    /// Get ACL connection
    #[must_use]
    pub fn get_connection(&self, handle: u16) -> Option<&AclConnection> {
        self.connections.get(&handle)
    }

    /// Get mutable ACL connection
    pub fn get_connection_mut(&mut self, handle: u16) -> Option<&mut AclConnection> {
        self.connections.get_mut(&handle)
    }

    /// Get reference to L2CAP processor
    #[must_use]
    pub fn l2cap_processor(&self) -> &SignalingProcessor {
        &self.l2cap_processor
    }

    /// Get mutable reference to L2CAP processor
    pub fn l2cap_processor_mut(&mut self) -> &mut SignalingProcessor {
        &mut self.l2cap_processor
    }

    /// Process incoming ACL data packet
    ///
    /// Handles fragmentation/reassembly and routes complete L2CAP packets
    /// to the signaling processor.
    ///
    /// # Errors
    /// Returns error if packet processing fails
    pub fn process_acl_packet(&mut self, packet: &AclPacket) -> Result<(), &'static str> {
        let handle = packet.header.connection_handle;

        // Get connection or return error
        let connection = self
            .get_connection_mut(handle)
            .ok_or("Unknown ACL connection handle")?;

        match packet.header.packet_boundary {
            PacketBoundary::CompletePdu => {
                // Complete L2CAP PDU - process directly
                self.process_l2cap_data(handle, &packet.data)?;
            }
            PacketBoundary::FirstNonFlushable | PacketBoundary::FirstFlushable => {
                // First fragment - reset buffer and start collecting
                connection.reset_fragmentation();

                // Extract expected length from L2CAP header if available
                if packet.data.len() >= 4 {
                    let l2cap_length = u16::from_le_bytes([packet.data[2], packet.data[3]]);
                    connection.expected_length = Some(l2cap_length + 4); // Include L2CAP header
                }

                connection.add_fragment(&packet.data)?;

                // Check if this fragment is already complete
                if connection.is_fragment_complete() {
                    // Clone the buffer to avoid borrowing issues
                    let complete_data = connection.fragment_buffer.clone();
                    connection.reset_fragmentation();
                    self.process_l2cap_data(handle, &complete_data)?;
                }
            }
            PacketBoundary::ContinuingFragment => {
                // Continuing fragment - add to buffer
                connection.add_fragment(&packet.data)?;

                // Check if fragmented packet is now complete
                if connection.is_fragment_complete() {
                    // Clone the buffer to avoid borrowing issues
                    let complete_data = connection.fragment_buffer.clone();
                    connection.reset_fragmentation();
                    self.process_l2cap_data(handle, &complete_data)?;
                }
            }
        }

        Ok(())
    }

    /// Process complete L2CAP data
    fn process_l2cap_data(&mut self, handle: u16, data: &[u8]) -> Result<(), &'static str> {
        // Parse L2CAP packet
        let l2cap_packet =
            L2capPacket::from_bytes(data).map_err(|_| "Failed to parse L2CAP packet")?;

        // Get connection info
        let connection = self.get_connection(handle).ok_or("Connection not found")?;

        // Route to L2CAP processor based on channel ID
        match l2cap_packet.header.channel_id {
            0x0001 => {
                // L2CAP signaling channel
                self.process_signaling_packet(&l2cap_packet, connection.remote_addr, handle)?;
            }
            0x0002 => {
                // Connectionless data channel - not implemented yet
                return Err("Connectionless data channel not supported");
            }
            0x0003 => {
                // AMP Manager Protocol - not implemented
                return Err("AMP Manager Protocol not supported");
            }
            0x0004 => {
                // Attribute Protocol - not implemented yet
                return Err("Attribute Protocol not supported");
            }
            0x0005 => {
                // LE L2CAP signaling channel - not implemented
                return Err("LE L2CAP signaling not supported");
            }
            0x0006 => {
                // Security Manager Protocol - not implemented
                return Err("Security Manager Protocol not supported");
            }
            0x0007 => {
                // BR/EDR Security Manager - not implemented
                return Err("BR/EDR Security Manager not supported");
            }
            cid if cid >= 0x0040 => {
                // Dynamic channel - route to appropriate handler
                self.process_dynamic_channel_packet(&l2cap_packet, connection.remote_addr, handle);
            }
            _ => {
                return Err("Reserved or invalid L2CAP channel ID");
            }
        }

        Ok(())
    }

    /// Process L2CAP signaling packet
    fn process_signaling_packet(
        &mut self,
        packet: &L2capPacket,
        remote_addr: BluetoothAddress,
        handle: u16,
    ) -> Result<(), &'static str> {
        use crate::l2cap::signaling::SignalingCommand;

        // Parse signaling command
        let command = SignalingCommand::from_bytes(&packet.payload)
            .map_err(|_| "Failed to parse signaling command")?;

        // Process command through L2CAP processor
        let response = self
            .l2cap_processor
            .handle_command(command, remote_addr, handle)
            .map_err(|_| "Failed to process signaling command")?;

        // If there's a response, we would send it back through HCI
        // For now, we just store it (actual transmission would be handled by HCI layer)
        if let Some(_response_command) = response {
            // TODO: Send response back through HCI layer
            // This would involve creating an ACL packet and sending it down to HCI
        }

        Ok(())
    }

    /// Process dynamic channel packet
    #[allow(clippy::unused_self)]
    fn process_dynamic_channel_packet(
        &mut self,
        _packet: &L2capPacket,
        _remote_addr: BluetoothAddress,
        _handle: u16,
    ) {
        // TODO: Route to appropriate service handler based on channel
        // For A2DP, this would route to AVDTP handler
    }

    /// Get number of active connections
    #[must_use]
    pub fn connection_count(&self) -> usize {
        self.connections.len()
    }
}

impl Default for AclManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_acl_header_parsing() {
        let bytes = [0x40, 0x30, 0x08, 0x00]; // Handle 0x0040, Complete PDU (bits 12-13 = 11), 8 bytes
        let header = AclHeader::from_bytes(&bytes).unwrap();

        assert_eq!(header.connection_handle, 0x0040);
        assert_eq!(header.packet_boundary, PacketBoundary::CompletePdu);
        assert_eq!(header.broadcast_flag, BroadcastFlag::PointToPoint);
        assert_eq!(header.data_length, 8);
    }

    #[test]
    fn test_acl_header_serialization() {
        let header = AclHeader::new(
            0x0040,
            PacketBoundary::CompletePdu,
            BroadcastFlag::PointToPoint,
            8,
        );

        let bytes = header.to_bytes();
        let parsed = AclHeader::from_bytes(&bytes).unwrap();

        assert_eq!(parsed, header);
    }

    #[test]
    fn test_acl_packet_complete() {
        let header = AclHeader::new(
            0x0040,
            PacketBoundary::CompletePdu,
            BroadcastFlag::PointToPoint,
            4,
        );

        let mut data: Vec<u8, 8> = Vec::new();
        data.extend_from_slice(&[0x01, 0x02, 0x03, 0x04]).unwrap();

        let packet = AclPacket::new(header, data);
        assert!(packet.is_complete_pdu());
        assert!(!packet.is_first_fragment());
        assert!(!packet.is_continuing_fragment());
    }

    #[test]
    fn test_acl_connection() {
        let addr = BluetoothAddress::new([0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC]);
        let mut conn = AclConnection::new(0x0040, addr);

        assert_eq!(conn.handle, 0x0040);
        assert_eq!(conn.remote_addr, addr);
        assert!(!conn.is_fragment_complete());

        // Add fragment
        conn.expected_length = Some(8);
        conn.add_fragment(&[0x01, 0x02, 0x03, 0x04]).unwrap();
        assert!(!conn.is_fragment_complete());

        conn.add_fragment(&[0x05, 0x06, 0x07, 0x08]).unwrap();
        assert!(conn.is_fragment_complete());

        let complete_data = conn.get_complete_data().unwrap();
        assert_eq!(
            complete_data,
            &[0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]
        );
    }

    #[test]
    fn test_acl_manager() {
        let mut manager = AclManager::new();
        let addr = BluetoothAddress::new([0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC]);

        // Add connection
        manager.add_connection(0x0040, addr).unwrap();
        assert_eq!(manager.connection_count(), 1);

        // Get connection
        let conn = manager.get_connection(0x0040).unwrap();
        assert_eq!(conn.handle, 0x0040);
        assert_eq!(conn.remote_addr, addr);

        // Remove connection
        let removed = manager.remove_connection(0x0040).unwrap();
        assert_eq!(removed.handle, 0x0040);
        assert_eq!(manager.connection_count(), 0);
    }

    #[test]
    fn test_packet_boundary_flags() {
        assert_eq!(
            PacketBoundary::from_u8(0x00),
            Some(PacketBoundary::FirstNonFlushable)
        );
        assert_eq!(
            PacketBoundary::from_u8(0x01),
            Some(PacketBoundary::ContinuingFragment)
        );
        assert_eq!(
            PacketBoundary::from_u8(0x02),
            Some(PacketBoundary::FirstFlushable)
        );
        assert_eq!(
            PacketBoundary::from_u8(0x03),
            Some(PacketBoundary::CompletePdu)
        );
        assert_eq!(
            PacketBoundary::from_u8(0x04),
            Some(PacketBoundary::FirstNonFlushable)
        ); // Wraps around
    }

    #[test]
    fn test_broadcast_flags() {
        assert_eq!(
            BroadcastFlag::from_u8(0x00),
            Some(BroadcastFlag::PointToPoint)
        );
        assert_eq!(
            BroadcastFlag::from_u8(0x01),
            Some(BroadcastFlag::ActiveBroadcast)
        );
        assert_eq!(
            BroadcastFlag::from_u8(0x02),
            Some(BroadcastFlag::PiconetBroadcast)
        );
        assert_eq!(BroadcastFlag::from_u8(0x03), Some(BroadcastFlag::Reserved));
    }
}
