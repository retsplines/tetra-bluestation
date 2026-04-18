/// 28.4.5.36 SNDCP version
#[derive(Debug, Clone)]
pub enum SNDCPVersion {
    /// Reserved
    Reserved = 0,

    /// Version 1 (which is the only version currently defined in the standard)
    Version1 = 1
}

impl TryFrom<u64> for SNDCPVersion {
    type Error = ();
    fn try_from(x: u64) -> Result<Self, Self::Error> {
        match x {
            0 => Ok(SNDCPVersion::Reserved),
            1 => Ok(SNDCPVersion::Version1),
            _ => Err(()),
        }
    }
}

impl SNDCPVersion {
    /// Convert this enum back into the raw integer value
    pub fn into_raw(self) -> u64 {
        match self {
            SNDCPVersion::Reserved => 0,
            SNDCPVersion::Version1 => 1,
        }
    }
}

impl From<SNDCPVersion> for u64 {
    fn from(e: SNDCPVersion) -> Self {
        e.into_raw()
    }
}

impl core::fmt::Display for SNDCPVersion {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SNDCPVersion::Reserved => write!(f, "Reserved"),
            SNDCPVersion::Version1 => write!(f, "Version 1"),
        }
    }
}
