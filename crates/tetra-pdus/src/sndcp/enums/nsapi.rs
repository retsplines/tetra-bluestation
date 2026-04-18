/// 28.4.5.22 NSAPI
#[derive(Debug, Clone)]
pub enum NSAPI {
    ReservedLow,
    DynamicallyAllocated(u8),
    ReservedHigh,
}

impl TryFrom<u64> for NSAPI {
    type Error = ();
    fn try_from(x: u64) -> Result<Self, Self::Error> {
        match x {
            0 => Ok(NSAPI::ReservedLow),
            1..=14 => Ok(NSAPI::DynamicallyAllocated(x as u8)),
            15 => Ok(NSAPI::ReservedHigh),
            _ => Err(()),
        }
    }
}

impl NSAPI {
    /// Convert this enum back into the raw integer value
    pub fn into_raw(self) -> u64 {
        match self {
            NSAPI::ReservedLow => 0,
            NSAPI::DynamicallyAllocated(x) => x as u64,
            NSAPI::ReservedHigh => 15,
        }
    }
}

impl From<NSAPI> for u64 {
    fn from(e: NSAPI) -> Self {
        e.into_raw()
    }
}

impl core::fmt::Display for NSAPI {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            NSAPI::ReservedLow => write!(f, "Reserved (0)"),
            NSAPI::DynamicallyAllocated(x) => write!(f, "Dynamically Allocated ({})", x),
            NSAPI::ReservedHigh => write!(f, "Reserved (15)"),
        }
    }
}
