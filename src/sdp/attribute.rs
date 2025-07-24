//! SDP Attribute Processing
//!
//! This module provides functionality for processing SDP attributes,
//! parsing attribute data, and managing attribute lists.

use super::{SdpError, record::DataElement};
use heapless::Vec;

/// Universal SDP Attribute IDs
///
/// These are standardized attribute IDs defined by the Bluetooth SIG.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum UniversalAttributeId {
    /// Service Record Handle
    ServiceRecordHandle = 0x0000,
    /// Service Class ID List
    ServiceClassIdList = 0x0001,
    /// Service Record State
    ServiceRecordState = 0x0002,
    /// Service ID
    ServiceId = 0x0003,
    /// Protocol Descriptor List
    ProtocolDescriptorList = 0x0004,
    /// Browse Group List
    BrowseGroupList = 0x0005,
    /// Language Based Attribute ID List
    LanguageBaseAttributeIdList = 0x0006,
    /// Service Info Time To Live
    ServiceInfoTimeToLive = 0x0007,
    /// Service Availability
    ServiceAvailability = 0x0008,
    /// Bluetooth Profile Descriptor List
    BluetoothProfileDescriptorList = 0x0009,
    /// Documentation URL
    DocumentationUrl = 0x000A,
    /// Client Executable URL
    ClientExecutableUrl = 0x000B,
    /// Icon URL
    IconUrl = 0x000C,
    /// Additional Protocol Descriptor Lists
    AdditionalProtocolDescriptorLists = 0x000D,
}

/// Language-Based Attribute IDs
///
/// These IDs are offsets added to the language base ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum LanguageAttributeOffset {
    /// Service Name
    ServiceName = 0x0000,
    /// Service Description  
    ServiceDescription = 0x0001,
    /// Provider Name
    ProviderName = 0x0002,
}

/// Standard Language Base ID for English
pub const ENGLISH_LANGUAGE_BASE_ID: u16 = 0x0100;

/// Attribute Range for filtering
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttributeRange {
    /// Start attribute ID (inclusive)
    pub start: u16,
    /// End attribute ID (inclusive)
    pub end: u16,
}

/// Attribute List Entry
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttributeEntry {
    /// Attribute ID
    pub id: u16,
    /// Attribute value
    pub value: DataElement,
}

/// Attribute List Parser
///
/// Parses SDP attribute lists from raw byte data.
#[derive(Debug)]
pub struct AttributeListParser {
    /// Current position in data
    position: usize,
    /// Total data length
    length: usize,
}

/// Attribute Filter
///
/// Filters attributes based on ID ranges and specific IDs.
#[derive(Debug, Clone)]
pub struct AttributeFilter {
    /// Allowed attribute ranges
    ranges: Vec<AttributeRange, 8>,
    /// Specific attribute IDs
    ids: Vec<u16, 16>,
}

impl UniversalAttributeId {
    /// Convert to u16 value
    #[must_use]
    pub const fn to_u16(self) -> u16 {
        self as u16
    }

    /// Create from u16 value
    #[must_use]
    pub const fn from_u16(value: u16) -> Option<Self> {
        match value {
            0x0000 => Some(Self::ServiceRecordHandle),
            0x0001 => Some(Self::ServiceClassIdList),
            0x0002 => Some(Self::ServiceRecordState),
            0x0003 => Some(Self::ServiceId),
            0x0004 => Some(Self::ProtocolDescriptorList),
            0x0005 => Some(Self::BrowseGroupList),
            0x0006 => Some(Self::LanguageBaseAttributeIdList),
            0x0007 => Some(Self::ServiceInfoTimeToLive),
            0x0008 => Some(Self::ServiceAvailability),
            0x0009 => Some(Self::BluetoothProfileDescriptorList),
            0x000A => Some(Self::DocumentationUrl),
            0x000B => Some(Self::ClientExecutableUrl),
            0x000C => Some(Self::IconUrl),
            0x000D => Some(Self::AdditionalProtocolDescriptorLists),
            _ => None,
        }
    }

    /// Check if attribute is mandatory for all service records
    #[must_use]
    pub const fn is_mandatory(self) -> bool {
        matches!(
            self,
            Self::ServiceRecordHandle
                | Self::ServiceClassIdList
                | Self::ServiceRecordState
                | Self::ServiceId
        )
    }
}

impl AttributeRange {
    /// Create new attribute range
    #[must_use]
    pub const fn new(start: u16, end: u16) -> Self {
        Self { start, end }
    }

    /// Check if attribute ID is in range
    #[must_use]
    pub const fn contains(&self, id: u16) -> bool {
        id >= self.start && id <= self.end
    }

    /// Get range size
    #[must_use]
    pub const fn size(&self) -> u16 {
        self.end.saturating_sub(self.start) + 1
    }
}

impl AttributeEntry {
    /// Create new attribute entry
    #[must_use]
    pub const fn new(id: u16, value: DataElement) -> Self {
        Self { id, value }
    }

    /// Get attribute size in bytes
    #[must_use]
    pub fn size(&self) -> usize {
        3 + self.value.encoded_size() // ID (2 bytes) + type descriptor (1 byte) + value
    }

    /// Check if this is a universal attribute
    #[must_use]
    pub const fn is_universal(&self) -> bool {
        UniversalAttributeId::from_u16(self.id).is_some()
    }

    /// Check if this is a language-based attribute
    #[must_use]
    pub const fn is_language_based(&self) -> bool {
        self.id >= ENGLISH_LANGUAGE_BASE_ID && self.id < (ENGLISH_LANGUAGE_BASE_ID + 0x100)
    }
}

impl AttributeListParser {
    /// Create new parser for attribute data
    #[must_use]
    pub const fn new(data_length: usize) -> Self {
        Self {
            position: 0,
            length: data_length,
        }
    }

    /// Parse next attribute from data
    ///
    /// # Errors
    /// Returns error if parsing fails or data is invalid
    pub fn parse_next(&mut self, data: &[u8]) -> Result<Option<AttributeEntry>, SdpError> {
        if self.position >= self.length || self.position >= data.len() {
            return Ok(None);
        }

        // Check minimum size for attribute ID (2 bytes)
        if self.position + 2 > data.len() {
            return Err(SdpError::InvalidData);
        }

        // Parse attribute ID (big-endian)
        let id = u16::from_be_bytes([data[self.position], data[self.position + 1]]);
        self.position += 2;

        // Parse data element
        let value = self.parse_data_element(data)?;

        Ok(Some(AttributeEntry::new(id, value)))
    }

    /// Parse data element from current position
    fn parse_data_element(&mut self, data: &[u8]) -> Result<DataElement, SdpError> {
        if self.position >= data.len() {
            return Err(SdpError::InvalidData);
        }

        let type_descriptor = data[self.position];
        self.position += 1;

        let data_type = (type_descriptor >> 3) & 0x1F;
        let size_index = type_descriptor & 0x07;

        match data_type {
            0 => Ok(DataElement::Nil),
            1 => {
                // Unsigned integer
                match size_index {
                    0 => {
                        if self.position >= data.len() {
                            return Err(SdpError::InvalidData);
                        }
                        let value = data[self.position];
                        self.position += 1;
                        Ok(DataElement::UnsignedInt8(value))
                    }
                    1 => {
                        if self.position + 2 > data.len() {
                            return Err(SdpError::InvalidData);
                        }
                        let value =
                            u16::from_be_bytes([data[self.position], data[self.position + 1]]);
                        self.position += 2;
                        Ok(DataElement::UnsignedInt16(value))
                    }
                    2 => {
                        if self.position + 4 > data.len() {
                            return Err(SdpError::InvalidData);
                        }
                        let value = u32::from_be_bytes([
                            data[self.position],
                            data[self.position + 1],
                            data[self.position + 2],
                            data[self.position + 3],
                        ]);
                        self.position += 4;
                        Ok(DataElement::UnsignedInt32(value))
                    }
                    _ => Err(SdpError::InvalidData),
                }
            }
            3 => {
                // UUID
                match size_index {
                    1 => {
                        if self.position + 2 > data.len() {
                            return Err(SdpError::InvalidData);
                        }
                        let value =
                            u16::from_be_bytes([data[self.position], data[self.position + 1]]);
                        self.position += 2;
                        Ok(DataElement::Uuid16(value))
                    }
                    2 => {
                        if self.position + 4 > data.len() {
                            return Err(SdpError::InvalidData);
                        }
                        let value = u32::from_be_bytes([
                            data[self.position],
                            data[self.position + 1],
                            data[self.position + 2],
                            data[self.position + 3],
                        ]);
                        self.position += 4;
                        Ok(DataElement::Uuid32(value))
                    }
                    4 => {
                        if self.position + 16 > data.len() {
                            return Err(SdpError::InvalidData);
                        }
                        let mut bytes = [0u8; 16];
                        bytes.copy_from_slice(&data[self.position..self.position + 16]);
                        self.position += 16;
                        Ok(DataElement::Uuid128(u128::from_be_bytes(bytes)))
                    }
                    _ => Err(SdpError::InvalidData),
                }
            }
            4 => {
                // Text string
                let length = self.parse_length(size_index, data)?;
                if self.position + length > data.len() {
                    return Err(SdpError::InvalidData);
                }

                let mut text = Vec::new();
                text.extend_from_slice(&data[self.position..self.position + length])
                    .map_err(|()| SdpError::TooManyServices)?;
                self.position += length;
                Ok(DataElement::TextString(text))
            }
            6 => {
                // Data element sequence - treating as UUID list for simplicity
                let length = self.parse_length(size_index, data)?;
                let mut uuids = Vec::new();
                let end_position = self.position + length;

                while self.position < end_position && self.position < data.len() {
                    // For now, assume all sequence elements are UUIDs (simplified)
                    // In a full implementation, this would need more sophisticated parsing
                    if self.position + 16 <= end_position {
                        let mut uuid_bytes = [0u8; 16];
                        uuid_bytes.copy_from_slice(&data[self.position..self.position + 16]);
                        let uuid = u128::from_be_bytes(uuid_bytes);
                        uuids.push(uuid).map_err(|_| SdpError::TooManyServices)?;
                        self.position += 16;
                    } else {
                        break;
                    }
                }

                Ok(DataElement::UuidList(uuids))
            }
            _ => Err(SdpError::InvalidData),
        }
    }

    /// Parse length field based on size index
    fn parse_length(&mut self, size_index: u8, data: &[u8]) -> Result<usize, SdpError> {
        match size_index {
            5 => {
                if self.position >= data.len() {
                    return Err(SdpError::InvalidData);
                }
                let length = data[self.position];
                self.position += 1;
                Ok(length as usize)
            }
            6 => {
                if self.position + 2 > data.len() {
                    return Err(SdpError::InvalidData);
                }
                let length = u16::from_be_bytes([data[self.position], data[self.position + 1]]);
                self.position += 2;
                Ok(length as usize)
            }
            7 => {
                if self.position + 4 > data.len() {
                    return Err(SdpError::InvalidData);
                }
                let length = u32::from_be_bytes([
                    data[self.position],
                    data[self.position + 1],
                    data[self.position + 2],
                    data[self.position + 3],
                ]);
                self.position += 4;
                Ok(length as usize)
            }
            _ => Err(SdpError::InvalidData),
        }
    }

    /// Reset parser position
    pub fn reset(&mut self) {
        self.position = 0;
    }

    /// Get current position
    #[must_use]
    pub const fn position(&self) -> usize {
        self.position
    }

    /// Check if parsing is complete
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        self.position >= self.length
    }
}

impl AttributeFilter {
    /// Create new empty filter
    #[must_use]
    pub fn new() -> Self {
        Self {
            ranges: Vec::new(),
            ids: Vec::new(),
        }
    }

    /// Add attribute range to filter
    ///
    /// # Errors
    /// Returns error if too many ranges are added
    pub fn add_range(&mut self, range: AttributeRange) -> Result<(), SdpError> {
        self.ranges
            .push(range)
            .map_err(|_| SdpError::TooManyServices)
    }

    /// Add specific attribute ID to filter
    ///
    /// # Errors
    /// Returns error if too many IDs are added
    pub fn add_id(&mut self, id: u16) -> Result<(), SdpError> {
        self.ids.push(id).map_err(|_| SdpError::TooManyServices)
    }

    /// Check if attribute ID passes filter
    #[must_use]
    pub fn matches(&self, id: u16) -> bool {
        // If no filters set, allow all
        if self.ranges.is_empty() && self.ids.is_empty() {
            return true;
        }

        // Check specific IDs
        if self.ids.contains(&id) {
            return true;
        }

        // Check ranges
        self.ranges.iter().any(|range| range.contains(id))
    }

    /// Clear all filters
    pub fn clear(&mut self) {
        self.ranges.clear();
        self.ids.clear();
    }

    /// Get number of ranges
    #[must_use]
    pub fn range_count(&self) -> usize {
        self.ranges.len()
    }

    /// Get number of specific IDs
    #[must_use]
    pub fn id_count(&self) -> usize {
        self.ids.len()
    }
}

impl Default for AttributeFilter {
    fn default() -> Self {
        Self::new()
    }
}

/// Create language-based attribute ID
#[must_use]
pub const fn language_attribute_id(base_id: u16, offset: LanguageAttributeOffset) -> u16 {
    base_id + offset as u16
}

/// Get English service name attribute ID
#[must_use]
pub const fn english_service_name_id() -> u16 {
    language_attribute_id(
        ENGLISH_LANGUAGE_BASE_ID,
        LanguageAttributeOffset::ServiceName,
    )
}

/// Get English service description attribute ID
#[must_use]
pub const fn english_service_description_id() -> u16 {
    language_attribute_id(
        ENGLISH_LANGUAGE_BASE_ID,
        LanguageAttributeOffset::ServiceDescription,
    )
}

/// Get English provider name attribute ID
#[must_use]
pub const fn english_provider_name_id() -> u16 {
    language_attribute_id(
        ENGLISH_LANGUAGE_BASE_ID,
        LanguageAttributeOffset::ProviderName,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_universal_attribute_id_conversion() {
        let attr = UniversalAttributeId::ServiceClassIdList;
        assert_eq!(attr.to_u16(), 0x0001);
        assert_eq!(UniversalAttributeId::from_u16(0x0001), Some(attr));
        assert_eq!(UniversalAttributeId::from_u16(0xFFFF), None);
    }

    #[test]
    fn test_mandatory_attributes() {
        assert!(UniversalAttributeId::ServiceRecordHandle.is_mandatory());
        assert!(UniversalAttributeId::ServiceClassIdList.is_mandatory());
        assert!(!UniversalAttributeId::DocumentationUrl.is_mandatory());
    }

    #[test]
    fn test_attribute_range() {
        let range = AttributeRange::new(0x0100, 0x01FF);
        assert!(range.contains(0x0150));
        assert!(!range.contains(0x0050));
        assert!(!range.contains(0x0250));
        assert_eq!(range.size(), 256);
    }

    #[test]
    fn test_attribute_entry() {
        let entry = AttributeEntry::new(0x0001, DataElement::UnsignedInt16(1234));
        assert!(entry.is_universal());
        assert!(!entry.is_language_based());

        let lang_entry = AttributeEntry::new(0x0100, DataElement::TextString(Vec::new()));
        assert!(!lang_entry.is_universal());
        assert!(lang_entry.is_language_based());
    }

    #[test]
    fn test_attribute_filter() {
        let mut filter = AttributeFilter::new();

        // Empty filter matches all
        assert!(filter.matches(0x1234));

        filter.add_id(0x0001).unwrap();
        assert!(filter.matches(0x0001));
        assert!(!filter.matches(0x0002));

        filter
            .add_range(AttributeRange::new(0x0100, 0x01FF))
            .unwrap();
        assert!(filter.matches(0x0150));
        assert!(!filter.matches(0x0050));

        assert_eq!(filter.id_count(), 1);
        assert_eq!(filter.range_count(), 1);

        filter.clear();
        assert_eq!(filter.id_count(), 0);
        assert_eq!(filter.range_count(), 0);
    }

    #[test]
    fn test_language_attribute_ids() {
        assert_eq!(english_service_name_id(), 0x0100);
        assert_eq!(english_service_description_id(), 0x0101);
        assert_eq!(english_provider_name_id(), 0x0102);
    }

    #[test]
    fn test_attribute_list_parser_creation() {
        let parser = AttributeListParser::new(100);
        assert_eq!(parser.position(), 0);
        assert!(!parser.is_complete());
    }

    #[test]
    fn test_parser_simple_uint16() {
        let mut parser = AttributeListParser::new(5);

        // Attribute ID 0x0001, UnsignedInt16 value 0x1234
        let data = [0x00, 0x01, 0x09, 0x12, 0x34];

        let entry = parser.parse_next(&data).unwrap().unwrap();
        assert_eq!(entry.id, 0x0001);
        assert_eq!(entry.value, DataElement::UnsignedInt16(0x1234));

        assert!(parser.is_complete());
    }

    #[test]
    fn test_parser_text_string() {
        let mut parser = AttributeListParser::new(8);

        // Attribute ID 0x0100, TextString "test"
        let data = [0x01, 0x00, 0x25, 0x04, b't', b'e', b's', b't'];

        let entry = parser.parse_next(&data).unwrap().unwrap();
        assert_eq!(entry.id, 0x0100);
        if let DataElement::TextString(text) = entry.value {
            assert_eq!(text.as_slice(), b"test");
        } else {
            panic!("Expected TextString");
        }
    }

    #[test]
    fn test_parser_invalid_data() {
        let mut parser = AttributeListParser::new(2);

        // Only attribute ID, no value
        let data = [0x00, 0x01];

        assert!(parser.parse_next(&data).is_err());
    }
}
