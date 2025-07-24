//! Service Discovery Protocol (SDP) Implementation
//!
//! This module provides a complete implementation of the Bluetooth Service Discovery Protocol,
//! allowing devices to advertise and discover services.

/// SDP Protocol Service Multiplexer (PSM) for L2CAP
pub const SDP_PSM: u16 = 0x0001;

/// Maximum number of services that can be registered
pub const MAX_SERVICES: usize = 16;

/// Service record handle type
pub type ServiceRecordHandle = u32;

/// Transaction ID for SDP requests/responses
pub type TransactionId = u16;

pub mod attribute;
pub mod client;
pub mod protocol;
pub mod record;
pub mod server;

// Re-export commonly used types
pub use attribute::{AttributeEntry, AttributeFilter, AttributeRange, UniversalAttributeId};
pub use client::{AttributeDiscoveryResult, RequestState, SdpClient, ServiceDiscoveryResult};
pub use record::{AttributeId, DataElement, ServiceClassId, ServiceRecord};
pub use server::{SdpServer, ServiceAttributeResult, ServiceSearchResult};

/// SDP Protocol Data Unit IDs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SdpPduId {
    /// Reserved PDU ID
    Reserved = 0x00,
    /// Error Response
    ErrorResponse = 0x01,
    /// Service Search Request
    ServiceSearchRequest = 0x02,
    /// Service Search Response
    ServiceSearchResponse = 0x03,
    /// Service Attribute Request
    ServiceAttributeRequest = 0x04,
    /// Service Attribute Response
    ServiceAttributeResponse = 0x05,
    /// Service Search Attribute Request
    ServiceSearchAttributeRequest = 0x06,
    /// Service Search Attribute Response
    ServiceSearchAttributeResponse = 0x07,
}

/// SDP Error Codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum SdpErrorCode {
    /// Invalid/unsupported SDP version
    InvalidVersion = 0x0001,
    /// Invalid Service Record Handle
    InvalidServiceRecordHandle = 0x0002,
    /// Invalid request syntax
    InvalidRequestSyntax = 0x0003,
    /// Invalid PDU size
    InvalidPduSize = 0x0004,
    /// Invalid continuation state
    InvalidContinuationState = 0x0005,
    /// Insufficient resources to satisfy request
    InsufficientResources = 0x0006,
}

/// SDP Error Types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SdpError {
    /// Buffer too small for operation
    BufferTooSmall,
    /// Too many services registered
    TooManyServices,
    /// Service not found
    ServiceNotFound,
    /// Invalid protocol data
    InvalidProtocolData,
    /// Protocol error from remote device
    ProtocolError(SdpErrorCode),
    /// Invalid attribute data
    InvalidData,
}

impl From<SdpErrorCode> for SdpError {
    fn from(code: SdpErrorCode) -> Self {
        Self::ProtocolError(code)
    }
}
