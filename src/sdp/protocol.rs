//! SDP Protocol Implementation
//!
//! This module implements the SDP protocol message formats and processing
//! for communication between SDP clients and servers over L2CAP.

use super::{
    AttributeId, SdpError, SdpErrorCode, SdpPduId, ServiceRecordHandle, TransactionId,
    server::{MAX_SDP_RESPONSE_SIZE, ServiceAttributeResult, ServiceSearchResult},
};
use heapless::Vec;

/// Maximum size of SDP PDU
pub const MAX_SDP_PDU_SIZE: usize = 1024;

/// Maximum number of UUIDs in a service search pattern
pub const MAX_SEARCH_UUIDS: usize = 8;

/// Maximum number of attribute IDs in a request
pub const MAX_ATTRIBUTE_IDS: usize = 16;

/// SDP PDU Header
///
/// All SDP messages start with this 5-byte header containing the PDU ID,
/// transaction ID, and parameter length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SdpPduHeader {
    /// PDU identifier
    pub pdu_id: SdpPduId,
    /// Transaction identifier
    pub transaction_id: TransactionId,
    /// Length of parameters following the header
    pub parameter_length: u16,
}

/// Service Search Request parameters
#[derive(Debug, Clone)]
pub struct ServiceSearchRequest {
    /// Service search pattern (list of UUIDs)
    pub service_search_pattern: Vec<u128, MAX_SEARCH_UUIDS>,
    /// Maximum number of service record handles to return
    pub maximum_service_record_count: u16,
    /// Continuation state (for handling large responses)
    pub continuation_state: Vec<u8, 16>,
}

/// Service Search Response parameters
#[derive(Debug, Clone)]
pub struct ServiceSearchResponse {
    /// Total number of matching service records
    pub total_service_record_count: u16,
    /// Current number of service record handles returned
    pub current_service_record_count: u16,
    /// Service record handles
    pub service_record_handles: Vec<ServiceRecordHandle, 16>,
    /// Continuation state
    pub continuation_state: Vec<u8, 16>,
}

/// Service Attribute Request parameters
#[derive(Debug, Clone)]
pub struct ServiceAttributeRequest {
    /// Service record handle
    pub service_record_handle: ServiceRecordHandle,
    /// Maximum number of bytes to return in response
    pub maximum_attribute_byte_count: u16,
    /// Attribute ID/range list
    pub attribute_id_list: Vec<AttributeId, MAX_ATTRIBUTE_IDS>,
    /// Continuation state
    pub continuation_state: Vec<u8, 16>,
}

/// Service Attribute Response parameters
#[derive(Debug, Clone)]
pub struct ServiceAttributeResponse {
    /// Length of attribute list
    pub attribute_list_byte_count: u16,
    /// Attribute list (encoded data elements)
    pub attribute_list: Vec<u8, MAX_SDP_RESPONSE_SIZE>,
    /// Continuation state
    pub continuation_state: Vec<u8, 16>,
}

/// Service Search Attribute Request parameters
#[derive(Debug, Clone)]
pub struct ServiceSearchAttributeRequest {
    /// Service search pattern (list of UUIDs)
    pub service_search_pattern: Vec<u128, MAX_SEARCH_UUIDS>,
    /// Maximum number of bytes to return
    pub maximum_attribute_byte_count: u16,
    /// Attribute ID/range list
    pub attribute_id_list: Vec<AttributeId, MAX_ATTRIBUTE_IDS>,
    /// Continuation state
    pub continuation_state: Vec<u8, 16>,
}

/// Service Search Attribute Response parameters
#[derive(Debug, Clone)]
pub struct ServiceSearchAttributeResponse {
    /// Length of attribute lists
    pub attribute_lists_byte_count: u16,
    /// Attribute lists (encoded data elements)
    pub attribute_lists: Vec<u8, MAX_SDP_RESPONSE_SIZE>,
    /// Continuation state
    pub continuation_state: Vec<u8, 16>,
}

/// Error Response parameters
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErrorResponse {
    /// Error code
    pub error_code: SdpErrorCode,
    /// Additional error information
    pub error_info: Option<u16>,
}

/// SDP Protocol Message
///
/// Represents a complete SDP protocol message with header and parameters.
#[derive(Debug, Clone)]
pub enum SdpMessage {
    /// Service Search Request
    ServiceSearchRequest(SdpPduHeader, ServiceSearchRequest),
    /// Service Search Response
    ServiceSearchResponse(SdpPduHeader, ServiceSearchResponse),
    /// Service Attribute Request
    ServiceAttributeRequest(SdpPduHeader, ServiceAttributeRequest),
    /// Service Attribute Response
    ServiceAttributeResponse(SdpPduHeader, ServiceAttributeResponse),
    /// Service Search Attribute Request
    ServiceSearchAttributeRequest(SdpPduHeader, ServiceSearchAttributeRequest),
    /// Service Search Attribute Response
    ServiceSearchAttributeResponse(SdpPduHeader, ServiceSearchAttributeResponse),
    /// Error Response
    ErrorResponse(SdpPduHeader, ErrorResponse),
}

impl SdpPduHeader {
    /// Create new PDU header
    #[must_use]
    pub const fn new(
        pdu_id: SdpPduId,
        transaction_id: TransactionId,
        parameter_length: u16,
    ) -> Self {
        Self {
            pdu_id,
            transaction_id,
            parameter_length,
        }
    }

    /// Encode header to bytes
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn encode(&self) -> [u8; 5] {
        [
            self.pdu_id as u8,
            (self.transaction_id >> 8) as u8,
            self.transaction_id as u8,
            (self.parameter_length >> 8) as u8,
            self.parameter_length as u8,
        ]
    }

    /// Decode header from bytes
    ///
    /// # Errors
    /// Returns error if buffer is too small or contains invalid data
    pub fn decode(data: &[u8]) -> Result<Self, SdpError> {
        if data.len() < 5 {
            return Err(SdpError::BufferTooSmall);
        }

        let pdu_id = match data[0] {
            0x01 => SdpPduId::ErrorResponse,
            0x02 => SdpPduId::ServiceSearchRequest,
            0x03 => SdpPduId::ServiceSearchResponse,
            0x04 => SdpPduId::ServiceAttributeRequest,
            0x05 => SdpPduId::ServiceAttributeResponse,
            0x06 => SdpPduId::ServiceSearchAttributeRequest,
            0x07 => SdpPduId::ServiceSearchAttributeResponse,
            _ => return Err(SdpError::ProtocolError(SdpErrorCode::InvalidRequestSyntax)),
        };

        let transaction_id = (u16::from(data[1]) << 8) | u16::from(data[2]);
        let parameter_length = (u16::from(data[3]) << 8) | u16::from(data[4]);

        Ok(Self {
            pdu_id,
            transaction_id,
            parameter_length,
        })
    }
}

impl ServiceSearchRequest {
    /// Create new service search request
    #[must_use]
    pub fn new(maximum_count: u16) -> Self {
        Self {
            service_search_pattern: Vec::new(),
            maximum_service_record_count: maximum_count,
            continuation_state: Vec::new(),
        }
    }

    /// Add UUID to search pattern
    ///
    /// # Errors
    /// Returns error if too many UUIDs
    pub fn add_uuid(&mut self, uuid: u128) -> Result<(), SdpError> {
        self.service_search_pattern
            .push(uuid)
            .map_err(|_| SdpError::BufferTooSmall)
    }

    /// Set continuation state
    ///
    /// # Errors
    /// Returns error if continuation state is too large
    pub fn set_continuation_state(&mut self, state: &[u8]) -> Result<(), SdpError> {
        self.continuation_state.clear();
        self.continuation_state
            .extend_from_slice(state)
            .map_err(|()| SdpError::BufferTooSmall)
    }
}

impl ServiceSearchResponse {
    /// Create response from search result
    #[must_use]
    pub fn from_search_result(result: ServiceSearchResult) -> Self {
        Self {
            total_service_record_count: result.total_count,
            current_service_record_count: u16::try_from(result.handles.len()).unwrap_or(0),
            service_record_handles: result.handles,
            continuation_state: Vec::new(),
        }
    }
}

impl ServiceAttributeRequest {
    /// Create new service attribute request
    #[must_use]
    pub fn new(handle: ServiceRecordHandle, max_bytes: u16) -> Self {
        Self {
            service_record_handle: handle,
            maximum_attribute_byte_count: max_bytes,
            attribute_id_list: Vec::new(),
            continuation_state: Vec::new(),
        }
    }

    /// Add attribute ID to request
    ///
    /// # Errors
    /// Returns error if too many attribute IDs
    pub fn add_attribute_id(&mut self, id: AttributeId) -> Result<(), SdpError> {
        self.attribute_id_list
            .push(id)
            .map_err(|_| SdpError::BufferTooSmall)
    }
}

impl ServiceAttributeResponse {
    /// Create response from attribute result
    #[must_use]
    pub fn from_attribute_result(result: ServiceAttributeResult) -> Self {
        Self {
            attribute_list_byte_count: result.size,
            attribute_list: result.attributes,
            continuation_state: Vec::new(),
        }
    }
}

impl ErrorResponse {
    /// Create new error response
    #[must_use]
    pub const fn new(error_code: SdpErrorCode) -> Self {
        Self {
            error_code,
            error_info: None,
        }
    }

    /// Create error response with additional info
    #[must_use]
    pub const fn with_info(error_code: SdpErrorCode, info: u16) -> Self {
        Self {
            error_code,
            error_info: Some(info),
        }
    }
}

impl SdpMessage {
    /// Get transaction ID from message
    #[must_use]
    pub const fn transaction_id(&self) -> TransactionId {
        match self {
            Self::ServiceSearchRequest(header, _)
            | Self::ServiceSearchResponse(header, _)
            | Self::ServiceAttributeRequest(header, _)
            | Self::ServiceAttributeResponse(header, _)
            | Self::ServiceSearchAttributeRequest(header, _)
            | Self::ServiceSearchAttributeResponse(header, _)
            | Self::ErrorResponse(header, _) => header.transaction_id,
        }
    }

    /// Get PDU ID from message
    #[must_use]
    pub const fn pdu_id(&self) -> SdpPduId {
        match self {
            Self::ServiceSearchRequest(header, _)
            | Self::ServiceSearchResponse(header, _)
            | Self::ServiceAttributeRequest(header, _)
            | Self::ServiceAttributeResponse(header, _)
            | Self::ServiceSearchAttributeRequest(header, _)
            | Self::ServiceSearchAttributeResponse(header, _)
            | Self::ErrorResponse(header, _) => header.pdu_id,
        }
    }

    /// Create error response message
    #[must_use]
    pub fn create_error_response(transaction_id: TransactionId, error_code: SdpErrorCode) -> Self {
        let header = SdpPduHeader::new(SdpPduId::ErrorResponse, transaction_id, 2);
        let error = ErrorResponse::new(error_code);
        Self::ErrorResponse(header, error)
    }

    /// Encode message to bytes
    ///
    /// # Errors
    /// Returns error if encoding fails or buffer is too small
    pub fn encode(&self, buffer: &mut [u8]) -> Result<usize, SdpError> {
        if buffer.len() < 5 {
            return Err(SdpError::BufferTooSmall);
        }

        let header_bytes = match self {
            Self::ServiceSearchRequest(header, _)
            | Self::ServiceSearchResponse(header, _)
            | Self::ServiceAttributeRequest(header, _)
            | Self::ServiceAttributeResponse(header, _)
            | Self::ServiceSearchAttributeRequest(header, _)
            | Self::ServiceSearchAttributeResponse(header, _)
            | Self::ErrorResponse(header, _) => header.encode(),
        };

        buffer[0..5].copy_from_slice(&header_bytes);

        // Simplified encoding - in a full implementation, this would properly
        // encode all the parameters according to SDP specification
        match self {
            Self::ErrorResponse(_, error) => {
                if buffer.len() < 7 {
                    return Err(SdpError::BufferTooSmall);
                }
                buffer[5] = (error.error_code as u16 >> 8) as u8;
                buffer[6] = error.error_code as u8;
                Ok(7)
            }
            _ => {
                // For other message types, we'd need to implement full parameter encoding
                Ok(5)
            }
        }
    }
}

/// SDP Protocol Processor
///
/// Processes SDP protocol messages and coordinates with the SDP server.
#[derive(Debug)]
pub struct SdpProtocol {
    /// Current transaction ID
    transaction_id: TransactionId,
}

impl SdpProtocol {
    /// Create new SDP protocol processor
    #[must_use]
    pub fn new() -> Self {
        Self { transaction_id: 1 }
    }

    /// Generate next transaction ID
    #[must_use]
    pub fn next_transaction_id(&mut self) -> TransactionId {
        let id = self.transaction_id;
        self.transaction_id = self.transaction_id.wrapping_add(1);
        id
    }

    /// Parse SDP message from bytes
    ///
    /// # Errors
    /// Returns error if parsing fails or message is invalid
    pub fn parse_message(&self, data: &[u8]) -> Result<SdpMessage, SdpError> {
        let header = SdpPduHeader::decode(data)?;

        if data.len() < 5 + header.parameter_length as usize {
            return Err(SdpError::ProtocolError(SdpErrorCode::InvalidPduSize));
        }

        // Simplified parsing - in a full implementation, this would properly
        // parse all parameter types according to SDP specification
        match header.pdu_id {
            SdpPduId::ServiceSearchRequest => {
                let request = ServiceSearchRequest::new(10); // Default max count
                Ok(SdpMessage::ServiceSearchRequest(header, request))
            }
            SdpPduId::ServiceAttributeRequest => {
                let request = ServiceAttributeRequest::new(0, 1024); // Default values
                Ok(SdpMessage::ServiceAttributeRequest(header, request))
            }
            SdpPduId::ErrorResponse => {
                if header.parameter_length < 2 {
                    return Err(SdpError::ProtocolError(SdpErrorCode::InvalidPduSize));
                }
                let error_code = match (u16::from(data[5]) << 8) | u16::from(data[6]) {
                    0x0001 => SdpErrorCode::InvalidVersion,
                    0x0002 => SdpErrorCode::InvalidServiceRecordHandle,
                    0x0003 => SdpErrorCode::InvalidRequestSyntax,
                    0x0004 => SdpErrorCode::InvalidPduSize,
                    0x0005 => SdpErrorCode::InvalidContinuationState,
                    0x0006 => SdpErrorCode::InsufficientResources,
                    _ => return Err(SdpError::ProtocolError(SdpErrorCode::InvalidRequestSyntax)),
                };
                let error = ErrorResponse::new(error_code);
                Ok(SdpMessage::ErrorResponse(header, error))
            }
            _ => Err(SdpError::ProtocolError(SdpErrorCode::InvalidRequestSyntax)),
        }
    }
}

impl Default for SdpProtocol {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pdu_header_encoding() {
        let header = SdpPduHeader::new(SdpPduId::ServiceSearchRequest, 0x1234, 0x5678);
        let encoded = header.encode();

        assert_eq!(encoded[0], SdpPduId::ServiceSearchRequest as u8);
        assert_eq!(encoded[1], 0x12);
        assert_eq!(encoded[2], 0x34);
        assert_eq!(encoded[3], 0x56);
        assert_eq!(encoded[4], 0x78);
    }

    #[test]
    fn test_pdu_header_decoding() {
        let data = [0x02, 0x12, 0x34, 0x56, 0x78];
        let header = SdpPduHeader::decode(&data).unwrap();

        assert_eq!(header.pdu_id, SdpPduId::ServiceSearchRequest);
        assert_eq!(header.transaction_id, 0x1234);
        assert_eq!(header.parameter_length, 0x5678);
    }

    #[test]
    fn test_service_search_request() {
        let mut request = ServiceSearchRequest::new(10);
        assert_eq!(request.maximum_service_record_count, 10);

        request.add_uuid(0x1234).unwrap();
        assert_eq!(request.service_search_pattern.len(), 1);
        assert_eq!(request.service_search_pattern[0], 0x1234);
    }

    #[test]
    fn test_error_response() {
        let error = ErrorResponse::new(SdpErrorCode::InvalidRequestSyntax);
        assert_eq!(error.error_code, SdpErrorCode::InvalidRequestSyntax);
        assert!(error.error_info.is_none());

        let error_with_info = ErrorResponse::with_info(SdpErrorCode::InvalidPduSize, 42);
        assert_eq!(error_with_info.error_info, Some(42));
    }

    #[test]
    fn test_message_transaction_id() {
        let header = SdpPduHeader::new(SdpPduId::ServiceSearchRequest, 0x1234, 0);
        let request = ServiceSearchRequest::new(10);
        let message = SdpMessage::ServiceSearchRequest(header, request);

        assert_eq!(message.transaction_id(), 0x1234);
        assert_eq!(message.pdu_id(), SdpPduId::ServiceSearchRequest);
    }

    #[test]
    fn test_protocol_transaction_id() {
        let mut protocol = SdpProtocol::new();

        let id1 = protocol.next_transaction_id();
        let id2 = protocol.next_transaction_id();

        assert_ne!(id1, id2);
        assert_eq!(id2, id1.wrapping_add(1));
    }
}
