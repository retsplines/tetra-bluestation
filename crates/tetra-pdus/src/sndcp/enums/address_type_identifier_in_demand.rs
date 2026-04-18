/// 28.4.5.4 Address type identifier in demand
#[derive(Debug, Clone, PartialEq)]
pub enum AddressTypeIdentifierInDemand {
    IPv4Static,
    IPv4Dynamic,
    IPv6,
    MobileIPv4ForeignAgentCareOfAddressRequested,
    MobileIPv4CoLocatedCareOfAddressRequested,
    PrimaryNSAPI,
    Reserved(u8),
}

impl TryFrom<u64> for AddressTypeIdentifierInDemand {
    type Error = ();
    fn try_from(x: u64) -> Result<Self, Self::Error> {
        match x {
            0 => Ok(AddressTypeIdentifierInDemand::IPv4Static),
            1 => Ok(AddressTypeIdentifierInDemand::IPv4Dynamic),
            2 => Ok(AddressTypeIdentifierInDemand::IPv6),
            3 => Ok(AddressTypeIdentifierInDemand::MobileIPv4ForeignAgentCareOfAddressRequested),
            4 => Ok(AddressTypeIdentifierInDemand::MobileIPv4CoLocatedCareOfAddressRequested),
            5 => Ok(AddressTypeIdentifierInDemand::PrimaryNSAPI),
            6..=255 => Ok(AddressTypeIdentifierInDemand::Reserved(x as u8)),
            _ => Err(()),
        }
    }
}

impl AddressTypeIdentifierInDemand {
    /// Convert this enum back into the raw integer value
    pub fn into_raw(self) -> u64 {
        match self {
            AddressTypeIdentifierInDemand::IPv4Static => 0,
            AddressTypeIdentifierInDemand::IPv4Dynamic => 1,
            AddressTypeIdentifierInDemand::IPv6 => 2,
            AddressTypeIdentifierInDemand::MobileIPv4ForeignAgentCareOfAddressRequested => 3,
            AddressTypeIdentifierInDemand::MobileIPv4CoLocatedCareOfAddressRequested => 4,
            AddressTypeIdentifierInDemand::PrimaryNSAPI => 5,
            AddressTypeIdentifierInDemand::Reserved(x) => x as u64,
        }
    }
}

impl From<AddressTypeIdentifierInDemand> for u64 {
    fn from(e: AddressTypeIdentifierInDemand) -> Self {
        e.into_raw()
    }
}

impl core::fmt::Display for AddressTypeIdentifierInDemand {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            AddressTypeIdentifierInDemand::IPv4Static => write!(f, "IPv4 Static"),
            AddressTypeIdentifierInDemand::IPv4Dynamic => write!(f, "IPv4 Dynamic"),
            AddressTypeIdentifierInDemand::IPv6 => write!(f, "IPv6"),
            AddressTypeIdentifierInDemand::MobileIPv4ForeignAgentCareOfAddressRequested => {
                write!(f, "Mobile IPv4 Foreign Agent Care-of-Address Requested")
            }
            AddressTypeIdentifierInDemand::MobileIPv4CoLocatedCareOfAddressRequested => {
                write!(f, "Mobile IPv4 Co-Located Care-of-Address Requested")
            }
            AddressTypeIdentifierInDemand::PrimaryNSAPI => write!(f, "Primary NSAPI"),
            AddressTypeIdentifierInDemand::Reserved(x) => write!(f, "Reserved ({})", x),
        }
    }
}