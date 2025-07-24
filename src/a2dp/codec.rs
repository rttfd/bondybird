//! Audio Codec Support for A2DP
//!
//! This module provides codec definitions and capabilities for A2DP audio streaming.
//! Currently supports SBC (Sub-band Coding) codec with plans for additional codecs.

use super::A2dpError;

/// Supported audio codec types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CodecType {
    /// SBC (Sub-Band Coding) - Mandatory codec for A2DP
    Sbc = 0x00,
    /// MPEG-1,2 Audio (MP3)
    Mpeg12Audio = 0x01,
    /// MPEG-2,4 AAC
    Mpeg24Aac = 0x02,
    /// Aptx codec
    Aptx = 0x04,
    /// Vendor-specific codec
    VendorSpecific = 0xFF,
}

/// Codec capabilities for different codec types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodecCapabilities {
    /// SBC codec capabilities
    Sbc(SbcCapabilities),
    /// Future codec support can be added here
    Other(OtherCodecCapabilities),
}

/// SBC (Sub-Band Coding) Codec Capabilities
///
/// SBC is the mandatory codec for A2DP and provides good audio quality
/// with reasonable computational requirements.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SbcCapabilities {
    /// Sampling frequency support (bitfield)
    pub sampling_frequencies: SbcSamplingFrequency,
    /// Channel mode support (bitfield)
    pub channel_modes: SbcChannelMode,
    /// Block length support (bitfield)
    pub block_lengths: SbcBlockLength,
    /// Subbands support (bitfield)
    pub subbands: SbcSubbands,
    /// Allocation method support (bitfield)
    pub allocation_methods: SbcAllocationMethod,
    /// Minimum bitpool value (2-250)
    pub min_bitpool: u8,
    /// Maximum bitpool value (2-250)
    pub max_bitpool: u8,
}

/// SBC Sampling Frequency Support (bitfield)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SbcSamplingFrequency(pub u8);

impl SbcSamplingFrequency {
    /// 16000 Hz
    pub const HZ_16000: u8 = 0x08;
    /// 32000 Hz
    pub const HZ_32000: u8 = 0x04;
    /// 44100 Hz
    pub const HZ_44100: u8 = 0x02;
    /// 48000 Hz
    pub const HZ_48000: u8 = 0x01;

    /// Create with all frequencies supported
    #[must_use]
    pub const fn all() -> Self {
        Self(Self::HZ_16000 | Self::HZ_32000 | Self::HZ_44100 | Self::HZ_48000)
    }

    /// Create with standard frequencies (44.1kHz and 48kHz)
    #[must_use]
    pub const fn standard() -> Self {
        Self(Self::HZ_44100 | Self::HZ_48000)
    }

    /// Check if frequency is supported
    #[must_use]
    pub const fn supports(&self, freq: u8) -> bool {
        (self.0 & freq) != 0
    }
}

/// SBC Channel Mode Support (bitfield)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SbcChannelMode(pub u8);

impl SbcChannelMode {
    /// Mono
    pub const MONO: u8 = 0x08;
    /// Dual Channel
    pub const DUAL_CHANNEL: u8 = 0x04;
    /// Stereo
    pub const STEREO: u8 = 0x02;
    /// Joint Stereo
    pub const JOINT_STEREO: u8 = 0x01;

    /// Create with all modes supported
    #[must_use]
    pub const fn all() -> Self {
        Self(Self::MONO | Self::DUAL_CHANNEL | Self::STEREO | Self::JOINT_STEREO)
    }

    /// Create with stereo modes only
    #[must_use]
    pub const fn stereo() -> Self {
        Self(Self::STEREO | Self::JOINT_STEREO)
    }

    /// Check if mode is supported
    #[must_use]
    pub const fn supports(&self, mode: u8) -> bool {
        (self.0 & mode) != 0
    }
}

/// SBC Block Length Support (bitfield)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SbcBlockLength(pub u8);

impl SbcBlockLength {
    /// 4 blocks
    pub const BLOCKS_4: u8 = 0x08;
    /// 8 blocks
    pub const BLOCKS_8: u8 = 0x04;
    /// 12 blocks
    pub const BLOCKS_12: u8 = 0x02;
    /// 16 blocks
    pub const BLOCKS_16: u8 = 0x01;

    /// Create with all block lengths supported
    #[must_use]
    pub const fn all() -> Self {
        Self(Self::BLOCKS_4 | Self::BLOCKS_8 | Self::BLOCKS_12 | Self::BLOCKS_16)
    }

    /// Create with standard block lengths (8 and 16)
    #[must_use]
    pub const fn standard() -> Self {
        Self(Self::BLOCKS_8 | Self::BLOCKS_16)
    }

    /// Check if block length is supported
    #[must_use]
    pub const fn supports(&self, blocks: u8) -> bool {
        (self.0 & blocks) != 0
    }
}

/// SBC Subbands Support (bitfield)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SbcSubbands(pub u8);

impl SbcSubbands {
    /// 4 subbands
    pub const SUBBANDS_4: u8 = 0x02;
    /// 8 subbands
    pub const SUBBANDS_8: u8 = 0x01;

    /// Create with all subbands supported
    #[must_use]
    pub const fn all() -> Self {
        Self(Self::SUBBANDS_4 | Self::SUBBANDS_8)
    }

    /// Create with 8 subbands only (better quality)
    #[must_use]
    pub const fn high_quality() -> Self {
        Self(Self::SUBBANDS_8)
    }

    /// Check if subband count is supported
    #[must_use]
    pub const fn supports(&self, subbands: u8) -> bool {
        (self.0 & subbands) != 0
    }
}

/// SBC Allocation Method Support (bitfield)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SbcAllocationMethod(pub u8);

impl SbcAllocationMethod {
    /// SNR allocation method
    pub const SNR: u8 = 0x02;
    /// Loudness allocation method
    pub const LOUDNESS: u8 = 0x01;

    /// Create with all allocation methods supported
    #[must_use]
    pub const fn all() -> Self {
        Self(Self::SNR | Self::LOUDNESS)
    }

    /// Create with loudness only (better for music)
    #[must_use]
    pub const fn loudness() -> Self {
        Self(Self::LOUDNESS)
    }

    /// Check if allocation method is supported
    #[must_use]
    pub const fn supports(&self, method: u8) -> bool {
        (self.0 & method) != 0
    }
}

/// Placeholder for other codec capabilities
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OtherCodecCapabilities {
    /// Codec type
    pub codec_type: CodecType,
    /// Raw capability data
    pub data: heapless::Vec<u8, 32>,
}

impl CodecCapabilities {
    /// Get the codec type
    #[must_use]
    pub const fn codec_type(&self) -> CodecType {
        match self {
            Self::Sbc(_) => CodecType::Sbc,
            Self::Other(other) => other.codec_type,
        }
    }

    /// Create SBC codec capabilities with default values
    #[must_use]
    pub fn sbc_default() -> Self {
        Self::Sbc(SbcCapabilities::default())
    }

    /// Validate codec capabilities
    ///
    /// # Errors
    /// Returns error if capabilities are invalid
    pub fn validate(&self) -> Result<(), A2dpError> {
        match self {
            Self::Sbc(sbc) => sbc.validate(),
            Self::Other(_) => Ok(()),
        }
    }

    /// Get encoded length of capabilities
    #[must_use]
    pub fn encoded_length(&self) -> usize {
        match self {
            Self::Sbc(_) => 6, // SBC capabilities are 6 bytes
            Self::Other(other) => other.data.len(),
        }
    }

    /// Encode capabilities to bytes
    ///
    /// # Errors
    /// Returns error if buffer is too small or encoding fails
    pub fn encode(&self, buffer: &mut [u8]) -> Result<usize, A2dpError> {
        match self {
            Self::Sbc(sbc) => sbc.encode(buffer),
            Self::Other(other) => {
                if buffer.len() < other.data.len() {
                    return Err(A2dpError::ConfigurationFailed);
                }
                buffer[..other.data.len()].copy_from_slice(&other.data);
                Ok(other.data.len())
            }
        }
    }
}

impl SbcCapabilities {
    /// Create SBC capabilities with high quality defaults
    #[must_use]
    pub const fn high_quality() -> Self {
        Self {
            sampling_frequencies: SbcSamplingFrequency::standard(),
            channel_modes: SbcChannelMode::stereo(),
            block_lengths: SbcBlockLength::standard(),
            subbands: SbcSubbands::high_quality(),
            allocation_methods: SbcAllocationMethod::loudness(),
            min_bitpool: 32,
            max_bitpool: 64,
        }
    }

    /// Validate SBC capabilities
    ///
    /// # Errors
    /// Returns error if bitpool values are invalid
    pub const fn validate(&self) -> Result<(), A2dpError> {
        if self.min_bitpool < 2 || self.min_bitpool > 250 {
            return Err(A2dpError::ConfigurationFailed);
        }
        if self.max_bitpool < 2 || self.max_bitpool > 250 {
            return Err(A2dpError::ConfigurationFailed);
        }
        if self.min_bitpool > self.max_bitpool {
            return Err(A2dpError::ConfigurationFailed);
        }
        Ok(())
    }

    /// Encode SBC capabilities to bytes
    ///
    /// # Errors
    /// Returns error if buffer is too small
    pub fn encode(&self, buffer: &mut [u8]) -> Result<usize, A2dpError> {
        if buffer.len() < 6 {
            return Err(A2dpError::ConfigurationFailed);
        }

        buffer[0] = self.sampling_frequencies.0;
        buffer[1] = self.channel_modes.0;
        buffer[2] = self.block_lengths.0;
        buffer[3] = self.subbands.0;
        buffer[4] = self.allocation_methods.0;
        buffer[5] = self.min_bitpool;
        buffer[6] = self.max_bitpool;

        Ok(7) // Actually 7 bytes for SBC
    }

    /// Check if this capability set is compatible with another
    #[must_use]
    pub const fn is_compatible_with(&self, other: &Self) -> bool {
        // Check if there's any overlap in supported features
        (self.sampling_frequencies.0 & other.sampling_frequencies.0) != 0
            && (self.channel_modes.0 & other.channel_modes.0) != 0
            && (self.block_lengths.0 & other.block_lengths.0) != 0
            && (self.subbands.0 & other.subbands.0) != 0
            && (self.allocation_methods.0 & other.allocation_methods.0) != 0
            && self.max_bitpool >= other.min_bitpool
            && self.min_bitpool <= other.max_bitpool
    }
}

impl Default for SbcCapabilities {
    fn default() -> Self {
        Self {
            sampling_frequencies: SbcSamplingFrequency::all(),
            channel_modes: SbcChannelMode::all(),
            block_lengths: SbcBlockLength::all(),
            subbands: SbcSubbands::all(),
            allocation_methods: SbcAllocationMethod::all(),
            min_bitpool: 2,
            max_bitpool: 64,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sbc_sampling_frequency() {
        let freq = SbcSamplingFrequency::standard();
        assert!(freq.supports(SbcSamplingFrequency::HZ_44100));
        assert!(freq.supports(SbcSamplingFrequency::HZ_48000));
        assert!(!freq.supports(SbcSamplingFrequency::HZ_16000));
    }

    #[test]
    fn test_sbc_capabilities_validation() {
        let mut caps = SbcCapabilities::default();
        assert!(caps.validate().is_ok());

        caps.min_bitpool = 1; // Invalid
        assert!(caps.validate().is_err());

        caps.min_bitpool = 32;
        caps.max_bitpool = 16; // min > max
        assert!(caps.validate().is_err());
    }

    #[test]
    fn test_sbc_compatibility() {
        let caps1 = SbcCapabilities::high_quality();
        let caps2 = SbcCapabilities::default();

        assert!(caps1.is_compatible_with(&caps2));
        assert!(caps2.is_compatible_with(&caps1));
    }

    #[test]
    fn test_codec_capabilities() {
        let sbc_caps = CodecCapabilities::sbc_default();
        assert_eq!(sbc_caps.codec_type(), CodecType::Sbc);
        assert!(sbc_caps.validate().is_ok());
        assert_eq!(sbc_caps.encoded_length(), 6);
    }

    #[test]
    fn test_sbc_encoding() {
        let caps = SbcCapabilities::high_quality();
        let mut buffer = [0u8; 10];
        let len = caps.encode(&mut buffer).unwrap();
        assert_eq!(len, 7);

        // Verify encoded values
        assert_eq!(buffer[0], caps.sampling_frequencies.0);
        assert_eq!(buffer[1], caps.channel_modes.0);
    }
}
