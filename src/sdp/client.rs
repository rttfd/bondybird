//! SDP Client Implementation
//!
//! This module provides SDP client functionality for discovering services
//! on remote Bluetooth devices.

use super::{
    SdpError, ServiceRecordHandle, TransactionId,
    protocol::{SdpMessage, SdpProtocol, ServiceAttributeRequest, ServiceSearchRequest},
};
use crate::BluetoothAddress;
use heapless::{FnvIndexMap, Vec};

/// Maximum number of pending SDP requests
pub const MAX_PENDING_REQUESTS: usize = 4;

/// SDP Client Request State
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestState {
    /// Request is pending
    Pending,
    /// Request completed successfully
    Completed,
    /// Request failed with error
    Failed,
    /// Request timed out
    TimedOut,
}

/// Pending SDP Request
#[derive(Debug, Clone)]
pub struct PendingRequest {
    /// Transaction ID
    pub transaction_id: TransactionId,
    /// Remote device address
    pub remote_addr: BluetoothAddress,
    /// Request state
    pub state: RequestState,
    /// Request type for response handling
    pub request_type: RequestType,
}

/// SDP Request Types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestType {
    /// Service search request
    ServiceSearch,
    /// Service attribute request
    ServiceAttribute,
    /// Service search attribute request
    ServiceSearchAttribute,
}

/// Service Discovery Result
#[derive(Debug, Clone)]
pub struct ServiceDiscoveryResult {
    /// Remote device address
    pub remote_addr: BluetoothAddress,
    /// Discovered service handles
    pub service_handles: Vec<ServiceRecordHandle, 16>,
    /// Total number of services found
    pub total_services: u16,
}

/// Service Attribute Discovery Result
#[derive(Debug, Clone)]
pub struct AttributeDiscoveryResult {
    /// Remote device address
    pub remote_addr: BluetoothAddress,
    /// Service record handle
    pub service_handle: ServiceRecordHandle,
    /// Retrieved attributes
    pub attributes: Vec<u8, 1024>,
    /// Size of attribute data
    pub attribute_size: u16,
}

/// SDP Client
///
/// Manages SDP service discovery requests to remote devices.
#[derive(Debug)]
pub struct SdpClient {
    /// SDP protocol handler
    protocol: SdpProtocol,
    /// Pending requests
    pending_requests: FnvIndexMap<TransactionId, PendingRequest, MAX_PENDING_REQUESTS>,
    /// Discovery results cache
    discovery_cache: FnvIndexMap<BluetoothAddress, ServiceDiscoveryResult, 8>,
}

impl PendingRequest {
    /// Create new pending request
    #[must_use]
    pub const fn new(
        transaction_id: TransactionId,
        remote_addr: BluetoothAddress,
        request_type: RequestType,
    ) -> Self {
        Self {
            transaction_id,
            remote_addr,
            state: RequestState::Pending,
            request_type,
        }
    }

    /// Mark request as completed
    pub fn complete(&mut self) {
        self.state = RequestState::Completed;
    }

    /// Mark request as failed
    pub fn fail(&mut self) {
        self.state = RequestState::Failed;
    }

    /// Mark request as timed out
    pub fn timeout(&mut self) {
        self.state = RequestState::TimedOut;
    }

    /// Check if request is finished (completed, failed, or timed out)
    #[must_use]
    pub const fn is_finished(&self) -> bool {
        matches!(
            self.state,
            RequestState::Completed | RequestState::Failed | RequestState::TimedOut
        )
    }
}

impl SdpClient {
    /// Create new SDP client
    #[must_use]
    pub fn new() -> Self {
        Self {
            protocol: SdpProtocol::new(),
            pending_requests: FnvIndexMap::new(),
            discovery_cache: FnvIndexMap::new(),
        }
    }

    /// Discover services on remote device
    ///
    /// # Errors
    /// Returns error if request cannot be created or sent
    pub fn discover_services(
        &mut self,
        remote_addr: BluetoothAddress,
        service_uuids: &[u128],
        max_records: u16,
    ) -> Result<TransactionId, SdpError> {
        let transaction_id = self.protocol.next_transaction_id();

        // Create service search request
        let mut request = ServiceSearchRequest::new(max_records);
        for &uuid in service_uuids {
            request.add_uuid(uuid)?;
        }

        // Store pending request
        let pending = PendingRequest::new(transaction_id, remote_addr, RequestType::ServiceSearch);
        self.pending_requests
            .insert(transaction_id, pending)
            .map_err(|_| SdpError::TooManyServices)?;

        // In a full implementation, this would send the request over L2CAP
        // For now, we just return the transaction ID
        Ok(transaction_id)
    }

    /// Get service attributes from remote device
    ///
    /// # Errors
    /// Returns error if request cannot be created or sent
    pub fn get_service_attributes(
        &mut self,
        remote_addr: BluetoothAddress,
        service_handle: ServiceRecordHandle,
        attribute_ids: &[u16],
        max_bytes: u16,
    ) -> Result<TransactionId, SdpError> {
        let transaction_id = self.protocol.next_transaction_id();

        // Create service attribute request
        let mut request = ServiceAttributeRequest::new(service_handle, max_bytes);
        for &attr_id in attribute_ids {
            request.add_attribute_id(attr_id)?;
        }

        // Store pending request
        let pending =
            PendingRequest::new(transaction_id, remote_addr, RequestType::ServiceAttribute);
        self.pending_requests
            .insert(transaction_id, pending)
            .map_err(|_| SdpError::TooManyServices)?;

        Ok(transaction_id)
    }

    /// Process SDP response message
    ///
    /// # Errors
    /// Returns error if response processing fails
    pub fn process_response(&mut self, message: SdpMessage) -> Result<(), SdpError> {
        let transaction_id = message.transaction_id();

        // Find and update pending request
        if let Some(pending) = self.pending_requests.get_mut(&transaction_id) {
            match message {
                SdpMessage::ServiceSearchResponse(_, response) => {
                    if pending.request_type == RequestType::ServiceSearch {
                        // Process service search response
                        let result = ServiceDiscoveryResult {
                            remote_addr: pending.remote_addr,
                            service_handles: response.service_record_handles,
                            total_services: response.total_service_record_count,
                        };

                        // Cache the result
                        self.discovery_cache
                            .insert(pending.remote_addr, result)
                            .ok();
                        pending.complete();
                    } else {
                        pending.fail();
                    }
                }
                SdpMessage::ServiceAttributeResponse(_, _response) => {
                    if pending.request_type == RequestType::ServiceAttribute {
                        // Process service attribute response
                        // In a full implementation, this would parse the attribute data
                        pending.complete();
                    } else {
                        pending.fail();
                    }
                }
                SdpMessage::ErrorResponse(_, _) => {
                    pending.fail();
                }
                _ => {
                    // Unexpected response type
                    pending.fail();
                }
            }
        }

        Ok(())
    }

    /// Get request state
    #[must_use]
    pub fn get_request_state(&self, transaction_id: TransactionId) -> Option<RequestState> {
        self.pending_requests
            .get(&transaction_id)
            .map(|req| req.state)
    }

    /// Get discovery result from cache
    #[must_use]
    pub fn get_discovery_result(
        &self,
        remote_addr: &BluetoothAddress,
    ) -> Option<&ServiceDiscoveryResult> {
        self.discovery_cache.get(remote_addr)
    }

    /// Remove completed request
    pub fn remove_request(&mut self, transaction_id: TransactionId) -> Option<PendingRequest> {
        self.pending_requests.remove(&transaction_id)
    }

    /// Get all pending requests
    pub fn pending_requests(&self) -> impl Iterator<Item = &PendingRequest> {
        self.pending_requests.values()
    }

    /// Cleanup timed out requests
    pub fn cleanup_timed_out_requests(&mut self) {
        // Mark old pending requests as timed out
        for request in self.pending_requests.values_mut() {
            if request.state == RequestState::Pending {
                // In a real implementation, this would check timestamps
                // For now, we just leave them as pending
            }
        }

        // Remove finished requests
        self.pending_requests.retain(|_, req| !req.is_finished());
    }

    /// Clear discovery cache
    pub fn clear_cache(&mut self) {
        self.discovery_cache.clear();
    }

    /// Get number of cached discovery results
    #[must_use]
    pub fn cache_size(&self) -> usize {
        self.discovery_cache.len()
    }

    /// Check if client has pending requests
    #[must_use]
    pub fn has_pending_requests(&self) -> bool {
        self.pending_requests
            .values()
            .any(|req| req.state == RequestState::Pending)
    }
}

impl Default for SdpClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::super::ServiceClassId;
    use super::*;

    #[test]
    fn test_sdp_client_creation() {
        let client = SdpClient::new();
        assert_eq!(client.cache_size(), 0);
        assert!(!client.has_pending_requests());
    }

    #[test]
    fn test_service_discovery_request() {
        let mut client = SdpClient::new();
        let remote_addr = BluetoothAddress::new([0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);

        let uuid = ServiceClassId::AudioSource.to_uuid();
        let transaction_id = client.discover_services(remote_addr, &[uuid], 10).unwrap();

        assert!(client.has_pending_requests());
        assert_eq!(
            client.get_request_state(transaction_id),
            Some(RequestState::Pending)
        );
    }

    #[test]
    fn test_attribute_request() {
        let mut client = SdpClient::new();
        let remote_addr = BluetoothAddress::new([0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);

        let transaction_id = client
            .get_service_attributes(remote_addr, 0x10000, &[0x0001, 0x0004], 1024)
            .unwrap();

        assert!(client.has_pending_requests());
        assert_eq!(
            client.get_request_state(transaction_id),
            Some(RequestState::Pending)
        );
    }

    #[test]
    fn test_pending_request_lifecycle() {
        let remote_addr = BluetoothAddress::new([0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);
        let mut request = PendingRequest::new(1, remote_addr, RequestType::ServiceSearch);

        assert_eq!(request.state, RequestState::Pending);
        assert!(!request.is_finished());

        request.complete();
        assert_eq!(request.state, RequestState::Completed);
        assert!(request.is_finished());

        request.fail();
        assert_eq!(request.state, RequestState::Failed);
        assert!(request.is_finished());
    }

    #[test]
    fn test_discovery_cache() {
        let mut client = SdpClient::new();
        let remote_addr = BluetoothAddress::new([0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);

        assert!(client.get_discovery_result(&remote_addr).is_none());

        let result = ServiceDiscoveryResult {
            remote_addr,
            service_handles: Vec::new(),
            total_services: 5,
        };

        client.discovery_cache.insert(remote_addr, result).ok();
        assert_eq!(client.cache_size(), 1);

        let cached = client.get_discovery_result(&remote_addr).unwrap();
        assert_eq!(cached.total_services, 5);

        client.clear_cache();
        assert_eq!(client.cache_size(), 0);
    }

    #[test]
    fn test_request_cleanup() {
        let mut client = SdpClient::new();
        let remote_addr = BluetoothAddress::new([0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);

        let uuid = ServiceClassId::SerialPort.to_uuid();
        let transaction_id = client.discover_services(remote_addr, &[uuid], 5).unwrap();

        assert!(client.has_pending_requests());

        // Mark as completed and remove
        if let Some(request) = client.pending_requests.get_mut(&transaction_id) {
            request.complete();
        }

        let removed = client.remove_request(transaction_id);
        assert!(removed.is_some());
        assert!(!client.has_pending_requests());
    }
}
