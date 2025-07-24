//! SDP Server Implementation
//!
//! This module provides the SDP server functionality for maintaining and
//! serving service records to remote SDP clients.

use super::{SdpError, ServiceRecord, ServiceRecordHandle, TransactionId};
use heapless::{FnvIndexMap, Vec};

/// Maximum size of SDP response buffer
pub const MAX_SDP_RESPONSE_SIZE: usize = 1024;

/// Maximum number of service handles in a response
pub const MAX_SERVICE_HANDLES: usize = 16;

/// SDP Server
///
/// Maintains a database of service records and handles SDP requests
/// from remote clients.
#[derive(Debug)]
pub struct SdpServer {
    /// Service record database
    records:
        FnvIndexMap<ServiceRecordHandle, ServiceRecord, { super::record::MAX_SERVICE_RECORDS }>,
    /// Next service record handle to assign
    next_handle: ServiceRecordHandle,
    /// Transaction ID counter
    next_transaction_id: TransactionId,
}

/// SDP Request Handler
///
/// Processes incoming SDP requests and generates appropriate responses.
#[derive(Debug)]
pub struct SdpRequestHandler {
    /// Reference to SDP server
    server: SdpServer,
}

/// Service Search Result
#[derive(Debug, Clone)]
pub struct ServiceSearchResult {
    /// Matching service record handles
    pub handles: Vec<ServiceRecordHandle, MAX_SERVICE_HANDLES>,
    /// Total number of matching services
    pub total_count: u16,
}

/// Service Attribute Result
#[derive(Debug, Clone)]
pub struct ServiceAttributeResult {
    /// Service attributes as encoded data elements
    pub attributes: Vec<u8, MAX_SDP_RESPONSE_SIZE>,
    /// Size of attribute data
    pub size: u16,
}

impl SdpServer {
    /// Create new SDP server
    #[must_use]
    pub fn new() -> Self {
        let mut server = Self {
            records: FnvIndexMap::new(),
            next_handle: 0x0001_0000, // Start from a non-zero value
            next_transaction_id: 1,
        };

        // Add default SDP server service record
        server.add_default_sdp_service().ok();
        server
    }

    /// Add service record to database
    ///
    /// # Errors
    /// Returns error if too many service records or invalid record
    pub fn add_service_record(
        &mut self,
        mut record: ServiceRecord,
    ) -> Result<ServiceRecordHandle, SdpError> {
        let handle = self.next_handle;
        self.next_handle = self.next_handle.wrapping_add(1);

        // Assign handle to record
        record.handle = handle;

        // Add service record handle attribute
        record.add_attribute(
            super::record::universal_attributes::SERVICE_RECORD_HANDLE,
            super::record::DataElement::UnsignedInt32(handle),
        )?;

        // Add service record state attribute
        record.add_attribute(
            super::record::universal_attributes::SERVICE_RECORD_STATE,
            super::record::DataElement::UnsignedInt32(record.state),
        )?;

        self.records
            .insert(handle, record)
            .map_err(|_| SdpError::TooManyServices)?;

        Ok(handle)
    }

    /// Remove service record
    pub fn remove_service_record(&mut self, handle: ServiceRecordHandle) -> Option<ServiceRecord> {
        self.records.remove(&handle)
    }

    /// Get service record by handle
    #[must_use]
    pub fn get_service_record(&self, handle: ServiceRecordHandle) -> Option<&ServiceRecord> {
        self.records.get(&handle)
    }

    /// Get mutable service record by handle
    pub fn get_service_record_mut(
        &mut self,
        handle: ServiceRecordHandle,
    ) -> Option<&mut ServiceRecord> {
        self.records.get_mut(&handle)
    }

    /// Search for services by UUID pattern
    #[must_use]
    pub fn search_services(&self, service_uuids: &[u128]) -> ServiceSearchResult {
        let mut handles = Vec::new();
        let mut total_count = 0u16;

        for record in self.records.values() {
            if service_uuids.is_empty() {
                // If no UUIDs specified, match all services
                if handles.push(record.handle).is_err() {
                    break;
                }
                total_count = total_count.saturating_add(1);
            } else {
                // Check if record matches any of the requested UUIDs
                for &uuid in service_uuids {
                    if Self::record_matches_uuid(record, uuid) {
                        if handles.push(record.handle).is_err() {
                            break;
                        }
                        total_count = total_count.saturating_add(1);
                        break;
                    }
                }
            }
        }

        ServiceSearchResult {
            handles,
            total_count,
        }
    }

    /// Get attributes for a service record
    ///
    /// # Errors
    /// Returns error if service not found or encoding fails
    pub fn get_service_attributes(
        &self,
        handle: ServiceRecordHandle,
        attribute_ids: &[u16],
    ) -> Result<ServiceAttributeResult, SdpError> {
        let record = self.records.get(&handle).ok_or(SdpError::ServiceNotFound)?;

        let mut attributes = Vec::new();
        let mut total_size = 0u16;

        for &attr_id in attribute_ids {
            if let Some(attr_value) = record.get_attribute(attr_id) {
                // Encode attribute ID and value
                // This is a simplified encoding - full SDP would use proper data element encoding
                let encoded_size = attr_value.encoded_size();
                if attributes.len() + encoded_size + 4 <= MAX_SDP_RESPONSE_SIZE {
                    // Add attribute ID (2 bytes) + length (2 bytes) + data
                    attributes.extend_from_slice(&attr_id.to_be_bytes()).ok();
                    attributes
                        .extend_from_slice(&u16::try_from(encoded_size).unwrap_or(0).to_be_bytes())
                        .ok();
                    // Simplified: just add some placeholder data
                    for _ in 0..encoded_size {
                        attributes.push(0).ok();
                    }
                    total_size =
                        total_size.saturating_add(u16::try_from(encoded_size).unwrap_or(0) + 4);
                } else {
                    break;
                }
            }
        }

        Ok(ServiceAttributeResult {
            attributes,
            size: total_size,
        })
    }

    /// Get all service record handles
    #[must_use]
    pub fn get_all_handles(
        &self,
    ) -> Vec<ServiceRecordHandle, { super::record::MAX_SERVICE_RECORDS }> {
        self.records.keys().copied().collect()
    }

    /// Get number of service records
    #[must_use]
    pub fn service_count(&self) -> usize {
        self.records.len()
    }

    /// Generate next transaction ID
    #[must_use]
    pub fn next_transaction_id(&mut self) -> TransactionId {
        let id = self.next_transaction_id;
        self.next_transaction_id = self.next_transaction_id.wrapping_add(1);
        id
    }

    /// Add default SDP server service record
    fn add_default_sdp_service(&mut self) -> Result<(), SdpError> {
        use super::ServiceClassId;
        use super::record::{DataElement, ServiceRecord};

        let mut record = ServiceRecord::new(0x0000, ServiceClassId::ServiceDiscoveryServer);

        // Set service name
        record.set_service_name("SDP Server")?;

        // Set L2CAP protocol (PSM 0x0001)
        record.add_l2cap_protocol(super::SDP_PSM)?;

        // Set version number list - simplified for now
        record.add_attribute(0x0200, DataElement::UnsignedInt16(0x0100))?; // Version 1.0

        self.add_service_record(record)?;
        Ok(())
    }

    /// Check if service record matches UUID
    fn record_matches_uuid(record: &ServiceRecord, uuid: u128) -> bool {
        use super::record::{DataElement, universal_attributes};

        // Check service class ID list
        if let Some(DataElement::UuidList(class_list)) =
            record.get_attribute(universal_attributes::SERVICE_CLASS_ID_LIST)
        {
            return class_list.contains(&uuid);
        }

        // Could also check protocol descriptor list and other UUID-containing attributes
        false
    }
}

impl Default for SdpServer {
    fn default() -> Self {
        Self::new()
    }
}

impl SdpRequestHandler {
    /// Create new request handler
    #[must_use]
    pub fn new(server: SdpServer) -> Self {
        Self { server }
    }

    /// Handle service search request
    ///
    /// # Errors
    /// Returns error if request processing fails
    pub fn handle_service_search(
        &self,
        service_uuids: &[u128],
        max_records: u16,
    ) -> Result<ServiceSearchResult, SdpError> {
        let mut result = self.server.search_services(service_uuids);

        // Limit results to requested maximum
        if result.handles.len() > max_records as usize {
            result.handles.truncate(max_records as usize);
        }

        Ok(result)
    }

    /// Handle service attribute request
    ///
    /// # Errors
    /// Returns error if service not found or attribute processing fails
    pub fn handle_service_attribute(
        &self,
        service_handle: ServiceRecordHandle,
        attribute_ids: &[u16],
        max_bytes: u16,
    ) -> Result<ServiceAttributeResult, SdpError> {
        let mut result = self
            .server
            .get_service_attributes(service_handle, attribute_ids)?;

        // Limit response size
        if result.size > max_bytes {
            let truncate_at = max_bytes as usize;
            result.attributes.truncate(truncate_at);
            result.size = max_bytes;
        }

        Ok(result)
    }

    /// Handle service search attribute request (combined search + attribute)
    ///
    /// # Errors
    /// Returns error if request processing fails  
    pub fn handle_service_search_attribute(
        &self,
        service_uuids: &[u128],
        attribute_ids: &[u16],
        max_bytes: u16,
    ) -> Result<Vec<ServiceAttributeResult, MAX_SERVICE_HANDLES>, SdpError> {
        let search_result = self.server.search_services(service_uuids);
        let mut results = Vec::new();
        let mut total_bytes = 0u16;

        for handle in &search_result.handles {
            if total_bytes >= max_bytes {
                break;
            }

            let remaining_bytes = max_bytes - total_bytes;
            if let Ok(attr_result) = self.server.get_service_attributes(*handle, attribute_ids) {
                let result_size = attr_result.size.min(remaining_bytes);
                let mut truncated_result = attr_result;

                if truncated_result.size > result_size {
                    truncated_result.attributes.truncate(result_size as usize);
                    truncated_result.size = result_size;
                }

                total_bytes = total_bytes.saturating_add(truncated_result.size);

                if results.push(truncated_result).is_err() {
                    break;
                }
            }
        }

        Ok(results)
    }

    /// Get reference to underlying server
    #[must_use]
    pub const fn server(&self) -> &SdpServer {
        &self.server
    }

    /// Get mutable reference to underlying server
    pub fn server_mut(&mut self) -> &mut SdpServer {
        &mut self.server
    }
}

#[cfg(test)]
mod tests {
    use super::super::ServiceClassId;
    use super::super::record::ServiceRecord;
    use super::*;

    #[test]
    fn test_sdp_server_creation() {
        let server = SdpServer::new();
        assert_eq!(server.service_count(), 1); // Default SDP service

        let handles = server.get_all_handles();
        assert_eq!(handles.len(), 1);
    }

    #[test]
    fn test_service_record_management() {
        let mut server = SdpServer::new();
        let initial_count = server.service_count();

        let record = ServiceRecord::new(0x10000, ServiceClassId::AudioSource);
        let handle = server.add_service_record(record).unwrap();

        assert_eq!(server.service_count(), initial_count + 1);
        assert!(server.get_service_record(handle).is_some());

        let removed = server.remove_service_record(handle);
        assert!(removed.is_some());
        assert_eq!(server.service_count(), initial_count);
    }

    #[test]
    fn test_service_search() {
        let mut server = SdpServer::new();

        let audio_record = ServiceRecord::new(0x10001, ServiceClassId::AudioSource);
        server.add_service_record(audio_record).unwrap();

        let serial_record = ServiceRecord::new(0x10002, ServiceClassId::SerialPort);
        server.add_service_record(serial_record).unwrap();

        // Search for audio source
        let audio_uuid = ServiceClassId::AudioSource.to_uuid();
        let result = server.search_services(&[audio_uuid]);
        assert!(result.total_count >= 1);

        // Search for all services
        let result = server.search_services(&[]);
        assert!(result.total_count >= 3); // SDP + Audio + Serial
    }

    #[test]
    fn test_request_handler() {
        let mut server = SdpServer::new();
        let record = ServiceRecord::new(0x10003, ServiceClassId::HandsFree);
        let _handle = server.add_service_record(record).unwrap();

        let handler = SdpRequestHandler::new(server);

        let hf_uuid = ServiceClassId::HandsFree.to_uuid();
        let result = handler.handle_service_search(&[hf_uuid], 10).unwrap();
        assert!(result.total_count >= 1);
    }

    #[test]
    fn test_transaction_id_generation() {
        let mut server = SdpServer::new();

        let id1 = server.next_transaction_id();
        let id2 = server.next_transaction_id();

        assert_ne!(id1, id2);
        assert_eq!(id2, id1.wrapping_add(1));
    }
}
