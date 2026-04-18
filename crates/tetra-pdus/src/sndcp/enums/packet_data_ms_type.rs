/// 28.4.5.26 Packet Data MS Type
#[derive(Debug, Clone)]
pub enum PacketDataMSType {
    TypeA,
    TypeB,
    TypeC,
    TypeD,
    Reserved(u8),
}

impl TryFrom<u64> for PacketDataMSType {
    type Error = ();
    fn try_from(x: u64) -> Result<Self, Self::Error> {
        match x {
            0 => Ok(PacketDataMSType::TypeA),
            1 => Ok(PacketDataMSType::TypeB),
            2 => Ok(PacketDataMSType::TypeC),
            3 => Ok(PacketDataMSType::TypeD),
            4..=15 => Ok(PacketDataMSType::Reserved(x as u8)),
            _ => Err(()),
        }
    }
}

impl PacketDataMSType {
    /// Convert this enum back into the raw integer value
    pub fn into_raw(self) -> u64 {
        match self {
            PacketDataMSType::TypeA => 0,
            PacketDataMSType::TypeB => 1,
            PacketDataMSType::TypeC => 2,
            PacketDataMSType::TypeD => 3,
            PacketDataMSType::Reserved(x) => x as u64,
        }
    }
}

impl From<PacketDataMSType> for u64 {
    fn from(e: PacketDataMSType) -> Self {
        e.into_raw()
    }
}

impl core::fmt::Display for PacketDataMSType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            PacketDataMSType::TypeA => write!(f, "Type A"),
            PacketDataMSType::TypeB => write!(f, "Type B"),
            PacketDataMSType::TypeC => write!(f, "Type C"),
            PacketDataMSType::TypeD => write!(f, "Type D"),
            PacketDataMSType::Reserved(x) => write!(f, "Reserved ({x})"),
        }
    }
}
