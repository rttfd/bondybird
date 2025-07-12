use crate::BluetoothError;

/// A Bluetooth Device Address (`BD_ADDR`) wrapper for type safety
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, defmt::Format)]
pub struct BluetoothAddress(pub [u8; 6]);

impl BluetoothAddress {
    /// Create a new Bluetooth address from bytes
    #[must_use]
    pub const fn new(addr: [u8; 6]) -> Self {
        Self(addr)
    }

    /// Get the raw address bytes
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 6] {
        &self.0
    }

    /// Format the address as a colon-separated hex string
    #[must_use]
    pub fn format_hex(&self) -> heapless::String<17> {
        let mut result = heapless::String::new();
        for (i, byte) in self.0.iter().enumerate() {
            if i > 0 {
                result.push(':').ok();
            }
            // Format byte as hex
            let hex_chars = [
                '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'A', 'B', 'C', 'D', 'E', 'F',
            ];
            result.push(hex_chars[(byte >> 4) as usize]).ok();
            result.push(hex_chars[(byte & 0x0F) as usize]).ok();
        }
        result
    }

    /// Parse a Bluetooth address from a colon-separated hex string
    ///
    /// # Returns
    /// - `Ok(BluetoothAddress)` if the string is valid
    /// - `Err(BluetoothError::InvalidParameter)` if the string is invalid
    ///
    /// # Errors
    /// Returns an error if the string is not exactly 17 characters long or contains invalid characters
    pub fn from_hex(hex: &str) -> Result<Self, BluetoothError> {
        if hex.len() != 17 || !hex.chars().all(|c| c.is_ascii_hexdigit() || c == ':') {
            return Err(BluetoothError::InvalidParameter);
        }

        let mut bytes = [0u8; 6];
        for (i, byte) in hex.split(':').enumerate() {
            if i >= 6 || byte.len() != 2 {
                return Err(BluetoothError::InvalidParameter);
            }
            bytes[i] =
                u8::from_str_radix(byte, 16).map_err(|_| BluetoothError::InvalidParameter)?;
        }
        Ok(Self(bytes))
    }
}

impl From<[u8; 6]> for BluetoothAddress {
    fn from(addr: [u8; 6]) -> Self {
        Self(addr)
    }
}

impl From<BluetoothAddress> for [u8; 6] {
    fn from(addr: BluetoothAddress) -> Self {
        addr.0
    }
}

impl From<BluetoothAddress> for bt_hci::param::BdAddr {
    fn from(addr: BluetoothAddress) -> Self {
        bt_hci::param::BdAddr::new(addr.0)
    }
}

impl From<BluetoothAddress> for heapless::String<17> {
    fn from(addr: BluetoothAddress) -> Self {
        addr.format_hex()
    }
}

impl TryFrom<&str> for BluetoothAddress {
    type Error = BluetoothError;

    fn try_from(hex: &str) -> Result<Self, Self::Error> {
        BluetoothAddress::from_hex(hex)
    }
}

impl TryFrom<&[u8]> for BluetoothAddress {
    type Error = BluetoothError;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        if bytes.len() == 6 {
            let mut addr = [0u8; 6];
            addr.copy_from_slice(bytes);
            Ok(BluetoothAddress(addr))
        } else {
            Err(BluetoothError::InvalidParameter)
        }
    }
}

impl TryFrom<bt_hci::param::BdAddr> for BluetoothAddress {
    type Error = BluetoothError;

    fn try_from(bd_addr: bt_hci::param::BdAddr) -> Result<Self, Self::Error> {
        bd_addr.raw().try_into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BluetoothDevice;
    use heapless::String;

    #[test]
    fn test_bluetooth_address_creation() {
        let addr = BluetoothAddress::new([0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC]);
        assert_eq!(addr.as_bytes(), &[0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC]);
    }

    #[test]
    fn test_bluetooth_address_format_hex() {
        let addr = BluetoothAddress::new([0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC]);
        let formatted = addr.format_hex();
        assert_eq!(formatted.as_str(), "12:34:56:78:9A:BC");
    }

    #[test]
    fn test_bluetooth_address_format_hex_edge_cases() {
        // Test zero address
        let addr_zero = BluetoothAddress::new([0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
        assert_eq!(addr_zero.format_hex().as_str(), "00:00:00:00:00:00");

        // Test max address
        let addr_max = BluetoothAddress::new([0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
        assert_eq!(addr_max.format_hex().as_str(), "FF:FF:FF:FF:FF:FF");

        // Test mixed case
        let addr_mixed = BluetoothAddress::new([0x0A, 0xB1, 0x2C, 0xD3, 0x4E, 0xF5]);
        assert_eq!(addr_mixed.format_hex().as_str(), "0A:B1:2C:D3:4E:F5");
    }

    #[test]
    fn test_bluetooth_address_conversions() {
        let bytes = [0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC];

        // Test From<[u8; 6]>
        let addr: BluetoothAddress = bytes.into();
        assert_eq!(addr.as_bytes(), &bytes);

        // Test Into<[u8; 6]>
        let converted_bytes: [u8; 6] = addr.into();
        assert_eq!(converted_bytes, bytes);

        // Test From<bt_hci::param::BdAddr>
        let bd_addr: bt_hci::param::BdAddr = addr.into();
        assert_eq!(bd_addr.raw(), bytes);

        // From<&str>
        let hex_str = "12:34:56:78:9A:BC";
        let addr_from_str: BluetoothAddress = hex_str.try_into().unwrap();
        assert_eq!(addr_from_str.as_bytes(), &bytes);

        // Into<heapless::String<17>>
        let hex_string: heapless::String<17> = addr.into();
        assert_eq!(hex_string.as_str(), "12:34:56:78:9A:BC");
    }

    #[test]
    fn test_bluetooth_address_try_from_slice() {
        // Test valid slice
        let bytes = &[0x12u8, 0x34u8, 0x56u8, 0x78u8, 0x9Au8, 0xBCu8][..];
        let addr = BluetoothAddress::try_from(bytes).unwrap();
        assert_eq!(addr.as_bytes(), &[0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC]);

        // Test invalid lengths
        let bytes_short = &[0x12u8, 0x34u8, 0x56u8][..];
        let bytes_long = &[
            0x12u8, 0x34u8, 0x56u8, 0x78u8, 0x9Au8, 0xBCu8, 0xDEu8, 0xF0u8,
        ][..];

        assert!(BluetoothAddress::try_from(bytes_short).is_err());
        assert!(BluetoothAddress::try_from(bytes_long).is_err());
    }

    #[test]
    fn test_bluetooth_device_name_handling() {
        let addr = BluetoothAddress::new([0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC]);

        // Test with various name lengths using the new string API
        let short_name_bytes = [
            b'H', b'i', 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0,
        ];
        let full_name_str = "TTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTT"; // 32 'T' characters
        let empty_name_bytes = [0u8; 32]; // Empty name

        let device_short = BluetoothDevice::new(addr).with_name_bytes(short_name_bytes);
        let device_full =
            BluetoothDevice::new(addr).with_name(String::try_from(full_name_str).unwrap());
        let device_empty = BluetoothDevice::new(addr).with_name_bytes(empty_name_bytes);

        assert_eq!(device_short.name, Some(String::try_from("Hi").unwrap()));
        assert_eq!(
            device_full.name,
            Some(String::try_from(full_name_str).unwrap())
        );
        assert_eq!(device_empty.name, Some(String::try_from("").unwrap()));
    }
}
