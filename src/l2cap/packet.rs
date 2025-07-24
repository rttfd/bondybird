//! L2CAP Packet Structures and Parsing
//!
//! This module implements the L2CAP (Logical Link Control and Adaptation Protocol) packet
//! format as defined in the Bluetooth Core Specification. L2CAP provides connection-oriented
//! and connectionless data services to upper layer protocols.

use heapless::Vec;

/// L2CAP packet parsing errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum L2capError {
    /// Packet data is too short (insufficient bytes for header or payload)
    InsufficientData,
    /// Payload exceeds buffer capacity
    PayloadTooLarge,
}

impl core::fmt::Display for L2capError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InsufficientData => write!(f, "Insufficient data for L2CAP packet"),
            Self::PayloadTooLarge => write!(f, "L2CAP payload exceeds buffer capacity"),
        }
    }
}

/// L2CAP Channel Identifier (CID)
///
/// Channel identifiers are used to distinguish between different L2CAP channels.
/// Some CIDs are reserved for specific purposes:
/// - 0x0000: Reserved, shall not be used
/// - 0x0001: L2CAP Signaling channel
/// - 0x0002: Connectionless reception channel
/// - 0x0003: AMP Manager Protocol
/// - 0x0004-0x003F: Reserved
/// - 0x0040-0xFFFF: Dynamically allocated
pub type ChannelId = u16;

/// L2CAP Protocol Service Multiplexer (PSM)
///
/// PSMs identify the upper layer protocol or application that should receive the data.
/// Well-known PSMs include:
/// - 0x0001: SDP (Service Discovery Protocol)
/// - 0x0003: RFCOMM
/// - 0x0019: AVDTP (Audio/Video Distribution Transport Protocol) - used for A2DP
pub type ProtocolServiceMultiplexer = u16;

/// L2CAP reserved channel identifiers
pub mod cid {
    use super::ChannelId;

    /// Reserved - shall not be used
    pub const NULL: ChannelId = 0x0000;
    /// L2CAP Signaling channel
    pub const SIGNALING: ChannelId = 0x0001;
    /// Connectionless reception channel
    pub const CONNECTIONLESS: ChannelId = 0x0002;
    /// AMP Manager Protocol
    pub const AMP_MANAGER: ChannelId = 0x0003;

    /// First dynamically allocated CID
    pub const DYNAMIC_START: ChannelId = 0x0040;
}

/// Well-known Protocol Service Multiplexers
pub mod psm {
    use super::ProtocolServiceMultiplexer;

    /// Service Discovery Protocol
    pub const SDP: ProtocolServiceMultiplexer = 0x0001;
    /// RFCOMM Protocol
    pub const RFCOMM: ProtocolServiceMultiplexer = 0x0003;
    /// Audio/Video Distribution Transport Protocol (A2DP)
    pub const AVDTP: ProtocolServiceMultiplexer = 0x0019;
}

/// L2CAP Basic Header
///
/// All L2CAP packets start with this 4-byte header containing the payload length
/// and destination channel identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct L2capHeader {
    /// Length of the payload (not including the header itself)
    pub length: u16,
    /// Channel identifier of the intended recipient
    pub channel_id: ChannelId,
}

impl L2capHeader {
    /// Create a new L2CAP header
    #[must_use]
    pub fn new(length: u16, channel_id: ChannelId) -> Self {
        Self { length, channel_id }
    }

    /// Parse L2CAP header from byte slice
    ///
    /// # Errors
    /// Returns `L2capError::InsufficientData` if the slice is less than 4 bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, L2capError> {
        if bytes.len() < 4 {
            return Err(L2capError::InsufficientData);
        }

        let length = u16::from_le_bytes([bytes[0], bytes[1]]);
        let channel_id = u16::from_le_bytes([bytes[2], bytes[3]]);

        Ok(Self::new(length, channel_id))
    }

    /// Convert header to bytes (little-endian)
    #[must_use]
    pub fn to_bytes(self) -> [u8; 4] {
        let mut bytes = [0u8; 4];
        bytes[0..2].copy_from_slice(&self.length.to_le_bytes());
        bytes[2..4].copy_from_slice(&self.channel_id.to_le_bytes());
        bytes
    }

    /// Size of the L2CAP header in bytes
    pub const HEADER_SIZE: usize = 4;
}

/// L2CAP Packet
///
/// Represents a complete L2CAP packet with header and payload.
/// The maximum payload size is limited to fit within typical MTU constraints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct L2capPacket<const N: usize = 1024> {
    /// L2CAP header
    pub header: L2capHeader,
    /// Packet payload
    pub payload: Vec<u8, N>,
}

impl<const N: usize> L2capPacket<N> {
    /// Create a new L2CAP packet
    ///
    /// # Panics
    /// Panics if payload length exceeds `u16::MAX`
    #[must_use]
    pub fn new(channel_id: ChannelId, payload: Vec<u8, N>) -> Self {
        let length = u16::try_from(payload.len()).expect("Payload too large for L2CAP packet");
        let header = L2capHeader::new(length, channel_id);
        Self { header, payload }
    }

    /// Parse L2CAP packet from byte slice
    ///
    /// # Errors
    /// Returns `L2capError` if:
    /// - The slice is too short for the header
    /// - The payload length exceeds the buffer capacity
    /// - The actual payload is shorter than indicated in the header
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, L2capError> {
        let header = L2capHeader::from_bytes(bytes)?;

        if bytes.len() < L2capHeader::HEADER_SIZE + header.length as usize {
            return Err(L2capError::InsufficientData);
        }

        let payload_bytes =
            &bytes[L2capHeader::HEADER_SIZE..L2capHeader::HEADER_SIZE + header.length as usize];
        let mut payload = Vec::new();

        if payload.extend_from_slice(payload_bytes).is_err() {
            return Err(L2capError::PayloadTooLarge);
        }

        Ok(Self { header, payload })
    }

    /// Convert packet to bytes
    ///
    /// Returns the complete packet as a byte vector including header and payload.
    ///
    /// # Errors
    /// Returns `L2capError::PayloadTooLarge` if the output buffer is too small
    pub fn to_bytes(&self) -> Result<Vec<u8, N>, L2capError> {
        let mut bytes = Vec::new();

        // Add header
        if bytes.extend_from_slice(&self.header.to_bytes()).is_err() {
            return Err(L2capError::PayloadTooLarge);
        }

        // Add payload
        if bytes.extend_from_slice(&self.payload).is_err() {
            return Err(L2capError::PayloadTooLarge);
        }

        Ok(bytes)
    }

    /// Get the total packet size (header + payload)
    #[must_use]
    pub fn total_size(&self) -> usize {
        L2capHeader::HEADER_SIZE + self.payload.len()
    }

    /// Check if this is a signaling packet (sent to signaling channel)
    #[must_use]
    pub fn is_signaling(&self) -> bool {
        self.header.channel_id == cid::SIGNALING
    }

    /// Check if this is a connectionless packet
    #[must_use]
    pub fn is_connectionless(&self) -> bool {
        self.header.channel_id == cid::CONNECTIONLESS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_l2cap_header_creation() {
        let header = L2capHeader::new(100, 0x0040);
        assert_eq!(header.length, 100);
        assert_eq!(header.channel_id, 0x0040);
    }

    #[test]
    fn test_l2cap_header_serialization() {
        let header = L2capHeader::new(0x1234, 0x5678);
        let bytes = header.to_bytes();
        assert_eq!(bytes, [0x34, 0x12, 0x78, 0x56]); // Little-endian

        let parsed = L2capHeader::from_bytes(&bytes).unwrap();
        assert_eq!(parsed, header);
    }

    #[test]
    fn test_l2cap_header_parsing_errors() {
        // Test insufficient data
        let short_bytes = [0x01, 0x02, 0x03]; // Only 3 bytes
        assert_eq!(
            L2capHeader::from_bytes(&short_bytes),
            Err(L2capError::InsufficientData)
        );
    }

    #[test]
    fn test_l2cap_packet_creation() {
        let mut payload = Vec::<u8, 64>::new();
        payload.push(0x01).unwrap();
        payload.push(0x02).unwrap();

        let packet = L2capPacket::<64>::new(cid::SIGNALING, payload.clone());
        assert_eq!(packet.header.channel_id, cid::SIGNALING);
        assert_eq!(packet.header.length, 2);
        assert_eq!(packet.payload, payload);
    }

    #[test]
    fn test_l2cap_packet_serialization() {
        let mut payload = Vec::<u8, 64>::new();
        payload.extend_from_slice(&[0xAA, 0xBB, 0xCC]).unwrap();

        let packet = L2capPacket::<64>::new(0x0040, payload);
        let bytes = packet.to_bytes().unwrap();

        // Expected: [length_lo, length_hi, cid_lo, cid_hi, payload...]
        assert_eq!(bytes[0..4], [0x03, 0x00, 0x40, 0x00]); // length=3, cid=0x0040
        assert_eq!(bytes[4..7], [0xAA, 0xBB, 0xCC]);

        let parsed = L2capPacket::<64>::from_bytes(&bytes).unwrap();
        assert_eq!(parsed, packet);
    }

    #[test]
    fn test_signaling_channel_detection() {
        let packet = L2capPacket::<64>::new(cid::SIGNALING, Vec::new());
        assert!(packet.is_signaling());
        assert!(!packet.is_connectionless());

        let packet = L2capPacket::<64>::new(cid::CONNECTIONLESS, Vec::new());
        assert!(!packet.is_signaling());
        assert!(packet.is_connectionless());
    }
}
