//! Class of Device (`CoD`) implementation for Bluetooth devices
//!
//! This module provides comprehensive parsing and human-readable formatting
//! of Bluetooth Class of Device fields according to the Bluetooth specification.
//!
//! ## Structure
//!
//! The Class of Device is a 24-bit field that consists of:
//! - Major Service Classes (bits 23-13): 11 bits indicating supported services
//! - Major Device Class (bits 12-8): 5 bits identifying device category  
//! - Minor Device Class (bits 7-2): 6 bits for device subcategory
//! - Format Type (bits 1-0): 2 bits (always 0b00)
//!
//! ## Usage
//!
//! ```rust
//! use bondybird::ClassOfDevice;
//!
//! // Parse from raw 24-bit value
//! let cod = ClassOfDevice::from_raw(0x000404);
//!
//! // Get human-readable description
//! println!("{}", cod); // "Audio/Video (Wearable headset device)"
//!
//! // Access individual components
//! let major = cod.major_device_class();
//! let minor = cod.minor_device_class();
//! let services = cod.major_service_classes();
//! ```

use core::fmt::Write;
use heapless::Vec;

/// Class of Device (`CoD`) indicating device type and capabilities
///
/// The Class of Device is a 24-bit field that indicates the type of device
/// and the services it provides. It consists of:
/// - Major Device Class (5 bits)
/// - Minor Device Class (6 bits)
/// - Major Service Classes (11 bits)
/// - Format Type (2 bits, always 0b00)
///
/// Based on the Bluetooth specification and `CoD` definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClassOfDevice {
    raw: u32,
}

impl ClassOfDevice {
    /// Create a `ClassOfDevice` from raw 24-bit value
    #[must_use]
    pub fn from_raw(raw: u32) -> Self {
        Self {
            raw: raw & 0xFF_FFFF,
        }
    }

    /// Get the raw 24-bit value
    #[must_use]
    pub fn raw(&self) -> u32 {
        self.raw
    }

    /// Get the Major Device Class (bits 12-8)
    #[must_use]
    pub fn major_device_class(&self) -> MajorDeviceClass {
        let major = (self.raw >> 8) & 0x1F;
        MajorDeviceClass::from_raw(major as u8)
    }

    /// Get the Minor Device Class (bits 7-2)
    #[must_use]
    pub fn minor_device_class(&self) -> u8 {
        ((self.raw >> 2) & 0x3F) as u8
    }

    /// Get the Major Service Classes (bits 23-13)
    #[must_use]
    pub fn major_service_classes(&self) -> MajorServiceClasses {
        let services = (self.raw >> 13) & 0x7FF;
        MajorServiceClasses::from_raw(services as u16)
    }

    /// Get the Format Type (bits 1-0, should always be 0b00)
    #[must_use]
    pub fn format_type(&self) -> u8 {
        (self.raw & 0x3) as u8
    }

    /// Get a human-readable description of the device
    #[must_use]
    pub fn description(&self) -> DeviceDescription<'static> {
        let major = self.major_device_class();
        let minor = self.minor_device_class();
        let services = self.major_service_classes();

        DeviceDescription {
            major_class: major.description(),
            minor_class: major.minor_class_description(minor),
            services: services.descriptions(),
        }
    }

    /// Convert the Class of Device to a heapless string
    ///
    /// # Panics
    ///
    /// This function will panic if the string cannot be formatted
    /// within the allocated size.
    ///
    #[must_use]
    pub fn to_string<const N: usize>(&self) -> heapless::String<N> {
        let mut buffer = heapless::String::<N>::new();
        write!(buffer, "{self}").unwrap();
        buffer
    }
}

impl core::fmt::Display for ClassOfDevice {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let desc = self.description();
        write!(f, "{}", desc.major_class)?;
        if let Some(minor) = desc.minor_class {
            write!(f, " ({minor})")?;
        }
        if !desc.services.is_empty() {
            write!(f, " - Services: ")?;
            for (i, service) in desc.services.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{service}")?;
            }
        }
        Ok(())
    }
}

/// Major Device Class enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MajorDeviceClass {
    /// Miscellaneous devices
    Miscellaneous = 0x00,
    /// Computer devices (desktop, server, laptop, etc.)
    Computer = 0x01,
    /// Phone devices (cellular, cordless, smartphone, etc.)
    Phone = 0x02,
    /// LAN/Network Access Point devices
    LanNetworkAccessPoint = 0x03,
    /// Audio/Video devices (headphones, speakers, microphones, etc.)
    AudioVideo = 0x04,
    /// Peripheral devices (keyboard, mouse, etc.)
    Peripheral = 0x05,
    /// Imaging devices (display, camera, scanner, printer)
    Imaging = 0x06,
    /// Wearable devices (watch, glasses, etc.)
    Wearable = 0x07,
    /// Toy devices (robot, vehicle, controller, etc.)
    Toy = 0x08,
    /// Health devices (monitor, scale, etc.)
    Health = 0x09,
    /// Uncategorized devices
    Uncategorized = 0x1F,
    /// Reserved or unknown device class
    Reserved(u8),
}

impl MajorDeviceClass {
    /// Create `MajorDeviceClass` from raw 5-bit value
    #[must_use]
    pub fn from_raw(raw: u8) -> Self {
        match raw {
            0x00 => Self::Miscellaneous,
            0x01 => Self::Computer,
            0x02 => Self::Phone,
            0x03 => Self::LanNetworkAccessPoint,
            0x04 => Self::AudioVideo,
            0x05 => Self::Peripheral,
            0x06 => Self::Imaging,
            0x07 => Self::Wearable,
            0x08 => Self::Toy,
            0x09 => Self::Health,
            0x1F => Self::Uncategorized,
            other => Self::Reserved(other),
        }
    }

    /// Get human-readable description
    #[must_use]
    pub fn description(&self) -> &'static str {
        match self {
            Self::Miscellaneous => "Miscellaneous",
            Self::Computer => "Computer",
            Self::Phone => "Phone",
            Self::LanNetworkAccessPoint => "LAN/Network Access Point",
            Self::AudioVideo => "Audio/Video",
            Self::Peripheral => "Peripheral",
            Self::Imaging => "Imaging",
            Self::Wearable => "Wearable",
            Self::Toy => "Toy",
            Self::Health => "Health",
            Self::Uncategorized => "Uncategorized",
            Self::Reserved(_) => "Reserved",
        }
    }

    /// Get minor device class description for this major class
    #[must_use]
    pub fn minor_class_description(&self, minor: u8) -> Option<&'static str> {
        match self {
            Self::Computer => match minor {
                0x00 => Some("Uncategorized"),
                0x01 => Some("Desktop workstation"),
                0x02 => Some("Server-class computer"),
                0x03 => Some("Laptop"),
                0x04 => Some("Handheld PC/PDA"),
                0x05 => Some("Palm-sized PC/PDA"),
                0x06 => Some("Wearable computer"),
                0x07 => Some("Tablet"),
                _ => None,
            },
            Self::Phone => match minor {
                0x00 => Some("Uncategorized"),
                0x01 => Some("Cellular"),
                0x02 => Some("Cordless"),
                0x03 => Some("Smartphone"),
                0x04 => Some("Wired modem or voice gateway"),
                0x05 => Some("Common ISDN access"),
                _ => None,
            },
            Self::AudioVideo => match minor {
                0x00 => Some("Uncategorized"),
                0x01 => Some("Wearable headset device"),
                0x02 => Some("Hands-free device"),
                0x04 => Some("Microphone"),
                0x05 => Some("Loudspeaker"),
                0x06 => Some("Headphones"),
                0x07 => Some("Portable audio"),
                0x08 => Some("Car audio"),
                0x09 => Some("Set-top box"),
                0x0A => Some("HiFi audio device"),
                0x0B => Some("VCR"),
                0x0C => Some("Video camera"),
                0x0D => Some("Camcorder"),
                0x0E => Some("Video monitor"),
                0x0F => Some("Video display and loudspeaker"),
                0x10 => Some("Video conferencing"),
                0x12 => Some("Gaming/toy"),
                _ => None,
            },
            Self::Peripheral => {
                let keyboard_pointing = (minor >> 4) & 0x3;
                let feel = minor & 0xF;

                match (keyboard_pointing, feel) {
                    (0, 0) => Some("Uncategorized"),
                    (1, _) => Some("Keyboard"),
                    (2, _) => Some("Pointing device"),
                    (3, _) => Some("Combo keyboard/pointing device"),
                    (0, 1) => Some("Joystick"),
                    (0, 2) => Some("Gamepad"),
                    (0, 3) => Some("Remote control"),
                    (0, 4) => Some("Sensing device"),
                    (0, 5) => Some("Digitizer tablet"),
                    (0, 6) => Some("Card reader"),
                    (0, 7) => Some("Digital pen"),
                    (0, 8) => Some("Handheld scanner"),
                    (0, 9) => Some("Handheld gestural input device"),
                    _ => None,
                }
            }
            Self::Imaging => {
                let mut parts = Vec::<&'static str, 4>::new();
                if (minor & 0x04) != 0 {
                    let _ = parts.push("Display");
                }
                if (minor & 0x08) != 0 {
                    let _ = parts.push("Camera");
                }
                if (minor & 0x10) != 0 {
                    let _ = parts.push("Scanner");
                }
                if (minor & 0x20) != 0 {
                    let _ = parts.push("Printer");
                }
                if parts.is_empty() {
                    Some("Uncategorized")
                } else {
                    // For simplicity, just return the first capability
                    Some(parts[0])
                }
            }
            Self::Wearable => match minor {
                0x01 => Some("Wristwatch"),
                0x02 => Some("Pager"),
                0x03 => Some("Jacket"),
                0x04 => Some("Helmet"),
                0x05 => Some("Glasses"),
                _ => None,
            },
            Self::Toy => match minor {
                0x01 => Some("Robot"),
                0x02 => Some("Vehicle"),
                0x03 => Some("Doll / Action figure"),
                0x04 => Some("Controller"),
                0x05 => Some("Game"),
                _ => None,
            },
            Self::Health => match minor {
                0x00 => Some("Undefined"),
                0x01 => Some("Blood pressure monitor"),
                0x02 => Some("Thermometer"),
                0x03 => Some("Weighing scale"),
                0x04 => Some("Glucose meter"),
                0x05 => Some("Pulse oximeter"),
                0x06 => Some("Heart/pulse rate monitor"),
                0x07 => Some("Health data display"),
                0x08 => Some("Step counter"),
                0x09 => Some("Body composition analyzer"),
                0x0A => Some("Peak flow monitor"),
                0x0B => Some("Medication monitor"),
                0x0C => Some("Knee prosthesis"),
                0x0D => Some("Ankle prosthesis"),
                0x0E => Some("Generic health manager"),
                0x0F => Some("Personal mobility device"),
                _ => None,
            },
            _ => None,
        }
    }
}

/// Major Service Classes bit field
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MajorServiceClasses {
    raw: u16,
}

impl MajorServiceClasses {
    /// Create from raw 11-bit value
    #[must_use]
    pub fn from_raw(raw: u16) -> Self {
        Self { raw: raw & 0x7FF }
    }

    /// Get raw value
    #[must_use]
    pub fn raw(&self) -> u16 {
        self.raw
    }

    /// Check if Limited Discoverable Mode is set
    #[must_use]
    pub fn limited_discoverable_mode(&self) -> bool {
        (self.raw & 0x001) != 0
    }

    /// Check if Positioning is supported
    #[must_use]
    pub fn positioning(&self) -> bool {
        (self.raw & 0x008) != 0
    }

    /// Check if Networking is supported
    #[must_use]
    pub fn networking(&self) -> bool {
        (self.raw & 0x010) != 0
    }

    /// Check if Rendering is supported
    #[must_use]
    pub fn rendering(&self) -> bool {
        (self.raw & 0x020) != 0
    }

    /// Check if Capturing is supported
    #[must_use]
    pub fn capturing(&self) -> bool {
        (self.raw & 0x040) != 0
    }

    /// Check if Object Transfer is supported
    #[must_use]
    pub fn object_transfer(&self) -> bool {
        (self.raw & 0x080) != 0
    }

    /// Check if Audio is supported
    #[must_use]
    pub fn audio(&self) -> bool {
        (self.raw & 0x100) != 0
    }

    /// Check if Telephony is supported
    #[must_use]
    pub fn telephony(&self) -> bool {
        (self.raw & 0x200) != 0
    }

    /// Check if Information is supported
    #[must_use]
    pub fn information(&self) -> bool {
        (self.raw & 0x400) != 0
    }

    /// Get descriptions of all active services
    #[must_use]
    pub fn descriptions(&self) -> Vec<&'static str, 9> {
        let mut services = Vec::new();

        if self.limited_discoverable_mode() {
            let _ = services.push("Limited Discoverable");
        }
        if self.positioning() {
            let _ = services.push("Positioning");
        }
        if self.networking() {
            let _ = services.push("Networking");
        }
        if self.rendering() {
            let _ = services.push("Rendering");
        }
        if self.capturing() {
            let _ = services.push("Capturing");
        }
        if self.object_transfer() {
            let _ = services.push("Object Transfer");
        }
        if self.audio() {
            let _ = services.push("Audio");
        }
        if self.telephony() {
            let _ = services.push("Telephony");
        }
        if self.information() {
            let _ = services.push("Information");
        }

        services
    }
}

/// Complete device description with major class, minor class, and services
#[derive(Debug, Clone)]
pub struct DeviceDescription<'a> {
    /// Major device class description
    pub major_class: &'a str,
    /// Minor device class description (if available)
    pub minor_class: Option<&'a str>,
    /// Active service descriptions
    pub services: Vec<&'a str, 9>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::fmt::Write;

    #[test]
    fn test_class_of_device_parsing() {
        // Use 0x000404 which represents:
        // - Major Service Classes: 0x000 (no services)
        // - Major Device Class: 0x04 (Audio/Video)
        // - Minor Device Class: 0x01 (Wearable headset device)
        // - Format Type: 0x00
        let cod = ClassOfDevice::from_raw(0x0000_0404);

        // Test major class
        assert_eq!(cod.major_device_class(), MajorDeviceClass::AudioVideo);

        // Test minor class
        assert_eq!(cod.minor_device_class(), 0x01);

        // Test service classes (should be empty for 0x000404)
        assert!(!cod.major_service_classes().audio());

        // Test human-readable format
        let desc = cod.description();
        assert_eq!(desc.major_class, "Audio/Video");
        assert_eq!(desc.minor_class, Some("Wearable headset device"));
    }

    #[test]
    fn test_class_of_device_display_trait() {
        use core::fmt::Write;

        let cod = ClassOfDevice::from_raw(0x0000_0404);

        // Test Display implementation in a no_std compatible way
        let mut buffer = heapless::String::<64>::new();
        write!(buffer, "{cod}").unwrap();
        assert_eq!(buffer.as_str(), "Audio/Video (Wearable headset device)");
    }

    #[test]
    fn test_major_device_class_from_raw() {
        assert_eq!(
            MajorDeviceClass::from_raw(0x00),
            MajorDeviceClass::Miscellaneous
        );
        assert_eq!(MajorDeviceClass::from_raw(0x01), MajorDeviceClass::Computer);
        assert_eq!(MajorDeviceClass::from_raw(0x02), MajorDeviceClass::Phone);
        assert_eq!(
            MajorDeviceClass::from_raw(0x03),
            MajorDeviceClass::LanNetworkAccessPoint
        );
        assert_eq!(
            MajorDeviceClass::from_raw(0x04),
            MajorDeviceClass::AudioVideo
        );
        assert_eq!(
            MajorDeviceClass::from_raw(0x05),
            MajorDeviceClass::Peripheral
        );
        assert_eq!(MajorDeviceClass::from_raw(0x06), MajorDeviceClass::Imaging);
        assert_eq!(MajorDeviceClass::from_raw(0x07), MajorDeviceClass::Wearable);
        assert_eq!(MajorDeviceClass::from_raw(0x08), MajorDeviceClass::Toy);
        assert_eq!(MajorDeviceClass::from_raw(0x09), MajorDeviceClass::Health);
        assert_eq!(
            MajorDeviceClass::from_raw(0x1F),
            MajorDeviceClass::Uncategorized
        );
        assert_eq!(
            MajorDeviceClass::from_raw(0x0A),
            MajorDeviceClass::Reserved(0x0A)
        );
    }

    #[test]
    fn test_major_device_class_descriptions() {
        assert_eq!(MajorDeviceClass::Computer.description(), "Computer");
        assert_eq!(MajorDeviceClass::Phone.description(), "Phone");
        assert_eq!(MajorDeviceClass::AudioVideo.description(), "Audio/Video");
        assert_eq!(MajorDeviceClass::Peripheral.description(), "Peripheral");
        assert_eq!(MajorDeviceClass::Health.description(), "Health");
        assert_eq!(MajorDeviceClass::Reserved(0x0A).description(), "Reserved");
    }

    #[test]
    fn test_minor_device_class_computer() {
        let computer = MajorDeviceClass::Computer;
        assert_eq!(
            computer.minor_class_description(0x00),
            Some("Uncategorized")
        );
        assert_eq!(
            computer.minor_class_description(0x01),
            Some("Desktop workstation")
        );
        assert_eq!(
            computer.minor_class_description(0x02),
            Some("Server-class computer")
        );
        assert_eq!(computer.minor_class_description(0x03), Some("Laptop"));
        assert_eq!(
            computer.minor_class_description(0x04),
            Some("Handheld PC/PDA")
        );
        assert_eq!(computer.minor_class_description(0x07), Some("Tablet"));
        assert_eq!(computer.minor_class_description(0xFF), None);
    }

    #[test]
    fn test_minor_device_class_phone() {
        let phone = MajorDeviceClass::Phone;
        assert_eq!(phone.minor_class_description(0x00), Some("Uncategorized"));
        assert_eq!(phone.minor_class_description(0x01), Some("Cellular"));
        assert_eq!(phone.minor_class_description(0x02), Some("Cordless"));
        assert_eq!(phone.minor_class_description(0x03), Some("Smartphone"));
        assert_eq!(
            phone.minor_class_description(0x04),
            Some("Wired modem or voice gateway")
        );
        assert_eq!(phone.minor_class_description(0xFF), None);
    }

    #[test]
    fn test_minor_device_class_audio_video() {
        let av = MajorDeviceClass::AudioVideo;
        assert_eq!(av.minor_class_description(0x00), Some("Uncategorized"));
        assert_eq!(
            av.minor_class_description(0x01),
            Some("Wearable headset device")
        );
        assert_eq!(av.minor_class_description(0x02), Some("Hands-free device"));
        assert_eq!(av.minor_class_description(0x06), Some("Headphones"));
        assert_eq!(av.minor_class_description(0x12), Some("Gaming/toy"));
        assert_eq!(av.minor_class_description(0xFF), None);
    }

    #[test]
    fn test_minor_device_class_peripheral() {
        let peripheral = MajorDeviceClass::Peripheral;

        // Test uncategorized
        assert_eq!(
            peripheral.minor_class_description(0x00),
            Some("Uncategorized")
        );

        // Test keyboard/pointing combinations
        assert_eq!(peripheral.minor_class_description(0x10), Some("Keyboard")); // 0001 0000
        assert_eq!(
            peripheral.minor_class_description(0x20),
            Some("Pointing device")
        ); // 0010 0000
        assert_eq!(
            peripheral.minor_class_description(0x30),
            Some("Combo keyboard/pointing device")
        ); // 0011 0000

        // Test feel values (lower 4 bits)
        assert_eq!(peripheral.minor_class_description(0x01), Some("Joystick"));
        assert_eq!(peripheral.minor_class_description(0x02), Some("Gamepad"));
        assert_eq!(
            peripheral.minor_class_description(0x03),
            Some("Remote control")
        );
        assert_eq!(
            peripheral.minor_class_description(0x09),
            Some("Handheld gestural input device")
        );
    }

    #[test]
    fn test_minor_device_class_imaging() {
        let imaging = MajorDeviceClass::Imaging;

        // Test uncategorized (no bits set)
        assert_eq!(imaging.minor_class_description(0x00), Some("Uncategorized"));

        // Test individual capabilities
        assert_eq!(imaging.minor_class_description(0x04), Some("Display")); // bit 2
        assert_eq!(imaging.minor_class_description(0x08), Some("Camera")); // bit 3
        assert_eq!(imaging.minor_class_description(0x10), Some("Scanner")); // bit 4
        assert_eq!(imaging.minor_class_description(0x20), Some("Printer")); // bit 5

        // Test combinations (should return first capability)
        assert_eq!(imaging.minor_class_description(0x0C), Some("Display")); // Display + Camera
    }

    #[test]
    fn test_minor_device_class_wearable() {
        let wearable = MajorDeviceClass::Wearable;
        assert_eq!(wearable.minor_class_description(0x01), Some("Wristwatch"));
        assert_eq!(wearable.minor_class_description(0x02), Some("Pager"));
        assert_eq!(wearable.minor_class_description(0x03), Some("Jacket"));
        assert_eq!(wearable.minor_class_description(0x04), Some("Helmet"));
        assert_eq!(wearable.minor_class_description(0x05), Some("Glasses"));
        assert_eq!(wearable.minor_class_description(0xFF), None);
    }

    #[test]
    fn test_minor_device_class_toy() {
        let toy = MajorDeviceClass::Toy;
        assert_eq!(toy.minor_class_description(0x01), Some("Robot"));
        assert_eq!(toy.minor_class_description(0x02), Some("Vehicle"));
        assert_eq!(
            toy.minor_class_description(0x03),
            Some("Doll / Action figure")
        );
        assert_eq!(toy.minor_class_description(0x04), Some("Controller"));
        assert_eq!(toy.minor_class_description(0x05), Some("Game"));
        assert_eq!(toy.minor_class_description(0xFF), None);
    }

    #[test]
    fn test_minor_device_class_health() {
        let health = MajorDeviceClass::Health;
        assert_eq!(health.minor_class_description(0x00), Some("Undefined"));
        assert_eq!(
            health.minor_class_description(0x01),
            Some("Blood pressure monitor")
        );
        assert_eq!(health.minor_class_description(0x02), Some("Thermometer"));
        assert_eq!(health.minor_class_description(0x03), Some("Weighing scale"));
        assert_eq!(
            health.minor_class_description(0x0F),
            Some("Personal mobility device")
        );
        assert_eq!(health.minor_class_description(0xFF), None);
    }

    #[test]
    fn test_major_service_classes() {
        let services = MajorServiceClasses::from_raw(0x000);
        assert!(!services.limited_discoverable_mode());
        assert!(!services.positioning());
        assert!(!services.networking());
        assert!(!services.rendering());
        assert!(!services.capturing());
        assert!(!services.object_transfer());
        assert!(!services.audio());
        assert!(!services.telephony());
        assert!(!services.information());
        assert!(services.descriptions().is_empty());

        // Test individual service bits
        let audio_service = MajorServiceClasses::from_raw(0x100);
        assert!(audio_service.audio());
        assert!(!audio_service.telephony());

        let telephony_service = MajorServiceClasses::from_raw(0x200);
        assert!(telephony_service.telephony());
        assert!(!telephony_service.audio());

        // Test multiple services
        let multi_service = MajorServiceClasses::from_raw(0x300); // Audio + Telephony
        assert!(multi_service.audio());
        assert!(multi_service.telephony());
        let descriptions = multi_service.descriptions();
        assert_eq!(descriptions.len(), 2);
        assert!(descriptions.contains(&"Audio"));
        assert!(descriptions.contains(&"Telephony"));
    }

    #[test]
    fn test_bit_field_parsing() {
        // Test a complex example: 0x2409
        // Services: (0x2409 >> 13) & 0x7FF = 0x01 = Limited Discoverable Mode
        // Major: (0x2409 >> 8) & 0x1F = 0x24 & 0x1F = 0x04 = AudioVideo
        // Minor: (0x2409 >> 2) & 0x3F = 0x902 & 0x3F = 0x02 = Hands-free device
        // Format: 0x2409 & 0x03 = 0x01
        let cod = ClassOfDevice::from_raw(0x2409);

        assert_eq!(cod.major_device_class(), MajorDeviceClass::AudioVideo);
        assert_eq!(cod.minor_device_class(), 0x02);
        assert_eq!(cod.format_type(), 0x01);

        let services = cod.major_service_classes();
        assert!(services.limited_discoverable_mode()); // bit 0 of services
        assert!(!services.object_transfer()); // bit 7 not set in this example
        assert!(!services.audio()); // bit 8 not set in this example

        let desc = cod.description();
        assert_eq!(desc.major_class, "Audio/Video");
        assert_eq!(desc.minor_class, Some("Hands-free device"));
        assert_eq!(desc.services.len(), 1);
        assert!(desc.services.contains(&"Limited Discoverable"));
    }

    #[test]
    fn test_format_type() {
        // Format type should always be 0 for current Bluetooth spec
        let cod1 = ClassOfDevice::from_raw(0x0000_0000);
        assert_eq!(cod1.format_type(), 0);

        let cod2 = ClassOfDevice::from_raw(0x0012_3456);
        assert_eq!(cod2.format_type(), 0x02); // bits 1-0 of 0x56 = 0b10

        let cod3 = ClassOfDevice::from_raw(0x0012_3457);
        assert_eq!(cod3.format_type(), 0x03); // bits 1-0 of 0x57 = 0b11
    }

    #[test]
    fn test_display_formatting() {
        // Test with no services
        let cod1 = ClassOfDevice::from_raw(0x0000_0404);
        let mut display1 = heapless::String::<64>::new();
        write!(display1, "{cod1}").unwrap();
        assert_eq!(display1.as_str(), "Audio/Video (Wearable headset device)");

        // Test with services
        let cod2 = ClassOfDevice::from_raw(0x0020_0404); // Audio service bit set
        let mut display2 = heapless::String::<128>::new();
        write!(display2, "{cod2}").unwrap();
        assert_eq!(
            display2.as_str(),
            "Audio/Video (Wearable headset device) - Services: Audio"
        );

        // Test with no minor class description
        let cod3 = ClassOfDevice::from_raw(0x0000_0400); // AudioVideo but minor=0x00 = Uncategorized
        let mut display3 = heapless::String::<64>::new();
        write!(display3, "{cod3}").unwrap();
        assert_eq!(display3.as_str(), "Audio/Video (Uncategorized)");
    }
}
