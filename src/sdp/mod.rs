//! Service Discovery Protocol (SDP) Implementation
//!
//! This module provides a complete implementation of the Bluetooth Service Discovery Protocol,
//! enabling devices to advertise services and discover services on remote devices.
//!
//! ## Overview
//!
//! SDP allows Bluetooth devices to:
//! - **Advertise Services**: Register service records describing available services
//! - **Discover Services**: Search for services on remote devices by UUID patterns
//! - **Query Attributes**: Retrieve detailed service information and configuration
//!
//! ## Architecture
//!
//! The implementation is organized into 6 specialized modules:
//!
//! - `server`: SDP server with service database management
//! - `client`: SDP client for remote service discovery
//! - `protocol`: Protocol message encoding/decoding and state management
//! - `record`: Service record structures and data elements
//! - `attribute`: Attribute parsing and universal attribute definitions
//!
//! ## Usage Examples
//!
//! ### Server Side - Advertising Services
//!
//! ```rust,no_run
//! use bondybird::sdp::{SdpServer, ServiceRecord, ServiceClassId};
//!
//! let mut server = SdpServer::new();
//! let mut audio_service = ServiceRecord::new(0x10001, ServiceClassId::AudioSource);
//! audio_service.set_service_name("My Audio Player").unwrap();
//! audio_service.add_l2cap_protocol(0x0019).unwrap(); // A2DP PSM
//! let handle = server.add_service_record(audio_service).unwrap();
//! ```
//!
//! ### Client Side - Discovering Services
//!
//! ```rust,no_run
//! use bondybird::sdp::{SdpClient, ServiceClassId};
//! use bondybird::BluetoothAddress;
//!
//! let mut client = SdpClient::new();
//! let remote_addr = BluetoothAddress::new([0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC]);
//! let audio_uuid = ServiceClassId::AudioSource.to_uuid();
//!
//! let transaction_id = client.discover_services(
//!     remote_addr,
//!     &[audio_uuid],
//!     10 // Max results
//! ).unwrap();
//! ```
//!
//! ## Protocol Details
//!
//! SDP operates over L2CAP channel with PSM 0x0001 and supports:
//! - Service Search: Find services matching UUID patterns
//! - Service Attribute: Retrieve attributes for specific services  
//! - Service Search Attribute: Combined search and attribute retrieval
//! - Error handling with standardized error codes
//!
//! All operations use transaction IDs for request/response correlation and support
//! continuation states for handling large response datasets.

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
