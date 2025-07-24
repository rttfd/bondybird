//! SDP Service Record Management
//!
//! This module provides data structures and functionality for managing
//! SDP service records, including service class definitions and data elements.

use super::{SdpError, ServiceRecordHandle};
use heapless::{FnvIndexMap, Vec};

/// Maximum number of attributes per service record
pub const MAX_ATTRIBUTES_PER_RECORD: usize = 32;

/// Maximum number of service records
pub const MAX_SERVICE_RECORDS: usize = 16;

/// Service UUID type (128-bit)
pub type ServiceUuid = u128;

/// Attribute ID type
pub type AttributeId = u16;

/// Universal attribute IDs module
pub mod universal_attributes {
    /// Service Record Handle
    pub const SERVICE_RECORD_HANDLE: u16 = 0x0000;
    /// Service Class ID List
    pub const SERVICE_CLASS_ID_LIST: u16 = 0x0001;
    /// Service Record State
    pub const SERVICE_RECORD_STATE: u16 = 0x0002;
    /// Service ID
    pub const SERVICE_ID: u16 = 0x0003;
    /// Protocol Descriptor List
    pub const PROTOCOL_DESCRIPTOR_LIST: u16 = 0x0004;
    /// Browse Group List
    pub const BROWSE_GROUP_LIST: u16 = 0x0005;
}

/// Standard Bluetooth Service Classes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum ServiceClassId {
    /// SDP Server Service
    ServiceDiscoveryServer = 0x1000,
    /// Browse Group Descriptor
    BrowseGroupDescriptor = 0x1001,
    /// Serial Port Profile
    SerialPort = 0x1101,
    /// LAN Access Using PPP
    LanAccessPpp = 0x1102,
    /// Dialup Networking
    DialupNetworking = 0x1103,
    /// Object Push Profile
    ObjectPush = 0x1105,
    /// File Transfer Profile
    FileTransfer = 0x1106,
    /// Headset Profile
    Headset = 0x1108,
    /// Audio Source
    AudioSource = 0x110A,
    /// Audio Sink
    AudioSink = 0x110B,
    /// A/V Remote Control Target
    AvRemoteControlTarget = 0x110C,
    /// Advanced Audio Distribution Profile
    AdvancedAudioDistribution = 0x110D,
    /// A/V Remote Control
    AvRemoteControl = 0x110E,
    /// Hands-Free Profile
    HandsFree = 0x111E,
    /// Hands-Free Audio Gateway
    HandsFreeAudioGateway = 0x111F,
    /// Human Interface Device
    HumanInterfaceDevice = 0x1124,
}

/// Data element type identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DataElementType {
    /// Nil (null value)
    Nil = 0,
    /// Unsigned integer
    UnsignedInt = 1,
    /// Signed integer
    SignedInt = 2,
    /// UUID
    Uuid = 3,
    /// Text string
    TextString = 4,
    /// Boolean
    Boolean = 5,
    /// Data element sequence
    Sequence = 6,
    /// Data element alternative
    Alternative = 7,
    /// URL
    Url = 8,
}

/// Data element size descriptor
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DataElementSize {
    /// 1 byte
    Size1 = 0,
    /// 2 bytes
    Size2 = 1,
    /// 4 bytes
    Size4 = 2,
    /// 8 bytes
    Size8 = 3,
    /// 16 bytes
    Size16 = 4,
    /// Additional 8-bit size descriptor follows
    AdditionalU8 = 5,
    /// Additional 16-bit size descriptor follows
    AdditionalU16 = 6,
    /// Additional 32-bit size descriptor follows
    AdditionalU32 = 7,
}

/// SDP Data Element
///
/// Represents a data element in an SDP service record attribute.
/// Data elements are the basic building blocks of SDP information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataElement {
    /// Nil (null value)
    Nil,
    /// Unsigned 8-bit integer
    UnsignedInt8(u8),
    /// Unsigned 16-bit integer
    UnsignedInt16(u16),
    /// Unsigned 32-bit integer
    UnsignedInt32(u32),
    /// Signed 8-bit integer
    SignedInt8(i8),
    /// Signed 16-bit integer
    SignedInt16(i16),
    /// Signed 32-bit integer
    SignedInt32(i32),
    /// 16-bit UUID
    Uuid16(u16),
    /// 32-bit UUID
    Uuid32(u32),
    /// 128-bit UUID
    Uuid128(u128),
    /// Text string (UTF-8)
    TextString(Vec<u8, 64>),
    /// Boolean value
    Boolean(bool),
    /// URL string
    Url(Vec<u8, 64>),
    /// Simple UUID list for service classes
    UuidList(Vec<u128, 8>),
}

/// Service Record Attribute
///
/// Associates an attribute ID with its data element value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceAttribute {
    /// Attribute identifier
    pub id: AttributeId,
    /// Attribute value
    pub value: DataElement,
}

/// Service Record
///
/// Contains all attributes for a service, including service class,
/// protocol information, and service-specific attributes.
#[derive(Debug, Clone)]
pub struct ServiceRecord {
    /// Service record handle (assigned by SDP server)
    pub handle: ServiceRecordHandle,
    /// Service attributes
    pub attributes: FnvIndexMap<AttributeId, DataElement, MAX_ATTRIBUTES_PER_RECORD>,
    /// Service record state (for change tracking)
    pub state: u32,
}

impl ServiceClassId {
    /// Convert to 128-bit UUID
    #[must_use]
    pub const fn to_uuid(self) -> ServiceUuid {
        // Bluetooth Base UUID: 00000000-0000-1000-8000-00805F9B34FB
        // Service UUIDs are 16-bit values placed in the first 32 bits
        0x0000_1000_8000_0080_5F9B_34FB_0000_0000 | ((self as u128) << 96)
    }

    /// Get service name
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::ServiceDiscoveryServer => "Service Discovery Server",
            Self::BrowseGroupDescriptor => "Browse Group Descriptor",
            Self::SerialPort => "Serial Port",
            Self::LanAccessPpp => "LAN Access Using PPP",
            Self::DialupNetworking => "Dialup Networking",
            Self::ObjectPush => "Object Push",
            Self::FileTransfer => "File Transfer",
            Self::Headset => "Headset",
            Self::AudioSource => "Audio Source",
            Self::AudioSink => "Audio Sink",
            Self::AvRemoteControlTarget => "A/V Remote Control Target",
            Self::AdvancedAudioDistribution => "Advanced Audio Distribution",
            Self::AvRemoteControl => "A/V Remote Control",
            Self::HandsFree => "Hands-Free",
            Self::HandsFreeAudioGateway => "Hands-Free Audio Gateway",
            Self::HumanInterfaceDevice => "Human Interface Device",
        }
    }
}

impl DataElement {
    /// Create a new text string data element
    ///
    /// # Errors
    /// Returns error if string is too long
    pub fn text_string(text: &str) -> Result<Self, SdpError> {
        let mut vec = Vec::new();
        vec.extend_from_slice(text.as_bytes())
            .map_err(|()| SdpError::TooManyServices)?;
        Ok(Self::TextString(vec))
    }

    /// Get the data element type
    #[must_use]
    pub const fn data_type(&self) -> DataElementType {
        match self {
            Self::Nil => DataElementType::Nil,
            Self::UnsignedInt8(_) | Self::UnsignedInt16(_) | Self::UnsignedInt32(_) => {
                DataElementType::UnsignedInt
            }
            Self::SignedInt8(_) | Self::SignedInt16(_) | Self::SignedInt32(_) => {
                DataElementType::SignedInt
            }
            Self::Uuid16(_) | Self::Uuid32(_) | Self::Uuid128(_) => DataElementType::Uuid,
            Self::TextString(_) => DataElementType::TextString,
            Self::Boolean(_) => DataElementType::Boolean,
            Self::Url(_) => DataElementType::Url,
            Self::UuidList(_) => DataElementType::Sequence,
        }
    }

    /// Get the encoded size of this data element
    #[must_use]
    pub fn encoded_size(&self) -> usize {
        1 + match self {
            Self::Nil => 0,
            Self::UnsignedInt8(_) | Self::SignedInt8(_) | Self::Boolean(_) => 1,
            Self::UnsignedInt16(_) | Self::SignedInt16(_) | Self::Uuid16(_) => 2,
            Self::UnsignedInt32(_) | Self::SignedInt32(_) | Self::Uuid32(_) => 4,
            Self::Uuid128(_) => 16,
            Self::TextString(text) => {
                let len = text.len();
                if len < 256 { 1 + len } else { 2 + len }
            }
            Self::Url(url) => {
                let len = url.len();
                if len < 256 { 1 + len } else { 2 + len }
            }
            Self::UuidList(uuids) => {
                let data_size = uuids.len() * 16; // Each UUID is 16 bytes
                if data_size < 256 {
                    1 + data_size
                } else {
                    2 + data_size
                }
            }
        }
    }
}

impl ServiceRecord {
    /// Create new service record with given handle and service class
    #[must_use]
    pub fn new(handle: ServiceRecordHandle, service_class: ServiceClassId) -> Self {
        let mut record = Self {
            handle,
            attributes: FnvIndexMap::new(),
            state: 1,
        };

        // Add mandatory service record handle
        record
            .attributes
            .insert(
                universal_attributes::SERVICE_RECORD_HANDLE,
                DataElement::UnsignedInt32(handle),
            )
            .ok();

        // Add mandatory service record state
        record
            .attributes
            .insert(
                universal_attributes::SERVICE_RECORD_STATE,
                DataElement::UnsignedInt32(record.state),
            )
            .ok();

        // Add mandatory service class ID list
        let mut uuid_list = Vec::new();
        uuid_list.push(service_class.to_uuid()).ok();

        record
            .attributes
            .insert(
                universal_attributes::SERVICE_CLASS_ID_LIST,
                DataElement::UuidList(uuid_list),
            )
            .ok();

        record
    }

    /// Add attribute to service record
    ///
    /// # Errors
    /// Returns error if too many attributes or invalid attribute
    pub fn add_attribute(&mut self, id: AttributeId, value: DataElement) -> Result<(), SdpError> {
        self.attributes
            .insert(id, value)
            .map_err(|_| SdpError::TooManyServices)?;

        // Update record state
        self.state = self.state.wrapping_add(1);
        Ok(())
    }

    /// Get attribute value
    #[must_use]
    pub fn get_attribute(&self, id: AttributeId) -> Option<&DataElement> {
        self.attributes.get(&id)
    }

    /// Check if record matches service class
    #[must_use]
    pub fn matches_service_class(&self, service_class: ServiceClassId) -> bool {
        if let Some(DataElement::UuidList(class_list)) =
            self.get_attribute(universal_attributes::SERVICE_CLASS_ID_LIST)
        {
            let target_uuid = service_class.to_uuid();
            class_list.contains(&target_uuid)
        } else {
            false
        }
    }

    /// Set service name
    ///
    /// # Errors
    /// Returns error if name is too long
    pub fn set_service_name(&mut self, name: &str) -> Result<(), SdpError> {
        const SERVICE_NAME_ATTRIBUTE: AttributeId = 0x0100;
        let name_element = DataElement::text_string(name)?;
        self.add_attribute(SERVICE_NAME_ATTRIBUTE, name_element)
    }

    /// Add L2CAP protocol descriptor
    ///
    /// # Errors
    /// Returns error if protocol information cannot be added
    pub fn add_l2cap_protocol(&mut self, psm: u16) -> Result<(), SdpError> {
        // For simplicity, just store the PSM as a 16-bit value
        self.add_attribute(
            universal_attributes::PROTOCOL_DESCRIPTOR_LIST,
            DataElement::UnsignedInt16(psm),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_class_uuid_conversion() {
        let audio_source = ServiceClassId::AudioSource;
        let uuid = audio_source.to_uuid();

        // AudioSource = 0x110A, so we expect it in the high 32 bits
        // Full UUID should be: 0000110A-0000-1000-8000-00805F9B34FB
        let expected = 0x0000_1000_8000_0080_5F9B_34FB_0000_0000 | (0x110A_u128 << 96);
        assert_eq!(uuid, expected);
    }

    #[test]
    fn test_data_element_text_string() {
        let text = DataElement::text_string("Hello World").unwrap();
        assert_eq!(text.data_type(), DataElementType::TextString);

        if let DataElement::TextString(bytes) = text {
            assert_eq!(bytes.as_slice(), b"Hello World");
        } else {
            panic!("Expected TextString");
        }
    }

    #[test]
    fn test_data_element_encoded_size() {
        assert_eq!(DataElement::Nil.encoded_size(), 1);
        assert_eq!(DataElement::UnsignedInt8(42).encoded_size(), 2);
        assert_eq!(DataElement::UnsignedInt16(1234).encoded_size(), 3);
        assert_eq!(DataElement::Uuid128(0x1234_5678).encoded_size(), 17);

        let text = DataElement::text_string("test").unwrap();
        assert_eq!(text.encoded_size(), 6); // 1 + 1 + 4
    }

    #[test]
    fn test_service_record_creation() {
        let record = ServiceRecord::new(0x10000, ServiceClassId::AudioSource);

        assert_eq!(record.handle, 0x10000);
        assert!(
            record
                .get_attribute(universal_attributes::SERVICE_RECORD_HANDLE)
                .is_some()
        );
        assert!(
            record
                .get_attribute(universal_attributes::SERVICE_CLASS_ID_LIST)
                .is_some()
        );
        assert!(record.matches_service_class(ServiceClassId::AudioSource));
        assert!(!record.matches_service_class(ServiceClassId::AudioSink));
    }

    #[test]
    fn test_service_record_attributes() {
        let mut record = ServiceRecord::new(0x10001, ServiceClassId::SerialPort);

        // Test adding custom attribute
        let custom_value = DataElement::UnsignedInt16(1234);
        record.add_attribute(0x1000, custom_value.clone()).unwrap();

        assert_eq!(record.get_attribute(0x1000), Some(&custom_value));
    }

    #[test]
    fn test_service_record_l2cap_protocol() {
        let mut record = ServiceRecord::new(0x10002, ServiceClassId::AudioSource);
        record.add_l2cap_protocol(0x0019).unwrap(); // A2DP PSM

        assert!(
            record
                .get_attribute(universal_attributes::PROTOCOL_DESCRIPTOR_LIST)
                .is_some()
        );
    }

    #[test]
    fn test_service_record_service_name() {
        let mut record = ServiceRecord::new(0x10003, ServiceClassId::HandsFree);
        record.set_service_name("My Hands-Free Service").unwrap();

        assert!(record.get_attribute(0x0100).is_some());
    }
}
