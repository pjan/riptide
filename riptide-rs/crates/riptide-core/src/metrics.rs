//! Numeric and time-related domain newtypes.

use crate::{CoreError, CoreResult};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{num::NonZeroU32, num::NonZeroUsize, time::Duration};

macro_rules! non_zero_u32_type {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(NonZeroU32);

        impl $name {
            /// Creates a new non-zero value.
            pub fn new(value: u32) -> CoreResult<Self> {
                match NonZeroU32::new(value) {
                    Some(value) => Ok(Self(value)),
                    None => Err(CoreError::ZeroValue {
                        type_name: stringify!($name),
                    }),
                }
            }

            /// Returns the stored value.
            pub fn get(self) -> u32 {
                self.0.get()
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_u32(self.get())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = u32::deserialize(deserializer)?;
                Self::new(value).map_err(serde::de::Error::custom)
            }
        }
    };
}

non_zero_u32_type!(
    /// Audio bit depth in bits per sample.
    BitDepth
);
non_zero_u32_type!(
    /// Audio sample rate in hertz.
    SampleRateHz
);
non_zero_u32_type!(
    /// One-based track number.
    TrackNumber
);
non_zero_u32_type!(
    /// One-based disc number.
    DiscNumber
);
non_zero_u32_type!(
    /// Count of items in a provider collection.
    ItemCount
);

/// Bounded concurrency limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ConcurrencyLimit(NonZeroUsize);

impl ConcurrencyLimit {
    /// Creates a new concurrency limit. Zero is rejected structurally.
    pub fn new(value: usize) -> CoreResult<Self> {
        match NonZeroUsize::new(value) {
            Some(value) => Ok(Self(value)),
            None => Err(CoreError::ZeroValue {
                type_name: "ConcurrencyLimit",
            }),
        }
    }

    /// Returns the stored limit.
    pub fn get(self) -> usize {
        self.0.get()
    }
}

/// Duration of a media item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MediaDuration(Duration);

impl MediaDuration {
    /// Creates a duration from whole seconds.
    pub fn from_secs(seconds: u64) -> Self {
        Self(Duration::from_secs(seconds))
    }

    /// Returns the underlying duration.
    pub fn as_duration(self) -> Duration {
        self.0
    }
}

impl Serialize for MediaDuration {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(self.0.as_secs())
    }
}

impl<'de> Deserialize<'de> for MediaDuration {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let seconds = u64::deserialize(deserializer)?;
        Ok(Self::from_secs(seconds))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_newtypes_reject_zero() {
        assert!(TrackNumber::new(0).is_err());
        assert!(DiscNumber::new(0).is_err());
        assert!(ItemCount::new(0).is_err());
        assert!(BitDepth::new(0).is_err());
        assert!(SampleRateHz::new(0).is_err());
        assert!(ConcurrencyLimit::new(0).is_err());
    }

    #[test]
    fn numeric_newtypes_accept_valid_values() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(TrackNumber::new(1)?.get(), 1);
        assert_eq!(BitDepth::new(24)?.get(), 24);
        assert_eq!(SampleRateHz::new(44_100)?.get(), 44_100);
        assert_eq!(ConcurrencyLimit::new(4)?.get(), 4);
        Ok(())
    }

    #[test]
    fn media_duration_preserves_seconds() {
        let duration = MediaDuration::from_secs(123);
        assert_eq!(duration.as_duration().as_secs(), 123);
    }
}
