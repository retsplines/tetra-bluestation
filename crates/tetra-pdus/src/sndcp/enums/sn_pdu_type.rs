use tetra_core::Direction;

/// 28.4.5.38 SN PDU Type
#[derive(Debug, Clone)]
pub enum SNPDUType {
    ActivatePdpContextDemand,
    ActivatePdpContextAccept,
    DeactivatePdpContextAccept,
    DeactivatePdpContextDemand,
    ActivatePdpContextReject,
    Unitdata,
    Data,
    DataTransmitRequest,
    DataTransmitResponse,
    EndOfData,
    Reconnect,
    PageRequest,
    PageResponse,
    NotSupported,
    DataPriority,
    Modify,
    Reserved
}

impl SNPDUType {

    pub fn from_raw(value: u64, direction: Direction) -> Result<Self, ()> {
        match value {
            0 => match direction {
                Direction::Ul => Ok(SNPDUType::ActivatePdpContextDemand),
                Direction::Dl => Ok(SNPDUType::ActivatePdpContextAccept),
                _ => Err(())
            },
            1 => Ok(SNPDUType::DeactivatePdpContextAccept),
            2 => Ok(SNPDUType::DeactivatePdpContextDemand),
            3 => Ok(SNPDUType::ActivatePdpContextReject),
            4 => Ok(SNPDUType::Unitdata),
            5 => Ok(SNPDUType::Data),
            6 => Ok(SNPDUType::DataTransmitRequest),
            7 => Ok(SNPDUType::DataTransmitResponse),
            8 => Ok(SNPDUType::EndOfData),
            9 => Ok(SNPDUType::Reconnect),
            10 => match direction {
                Direction::Dl => Ok(SNPDUType::PageRequest),
                Direction::Ul => Ok(SNPDUType::PageResponse),
                _ => Err(())
            },
            11 => Ok(SNPDUType::NotSupported),
            12 => Ok(SNPDUType::DataPriority),
            13 => Ok(SNPDUType::Modify),
            _ => Ok(SNPDUType::Reserved)
        }
    }

    pub fn into_raw(self) -> u64 {
        match self {
            SNPDUType::ActivatePdpContextDemand => 0,
            SNPDUType::ActivatePdpContextAccept => 0,
            SNPDUType::DeactivatePdpContextAccept => 1,
            SNPDUType::DeactivatePdpContextDemand => 2,
            SNPDUType::ActivatePdpContextReject => 3,
            SNPDUType::Unitdata => 4,
            SNPDUType::Data => 5,
            SNPDUType::DataTransmitRequest => 6,
            SNPDUType::DataTransmitResponse => 7,
            SNPDUType::EndOfData => 8,
            SNPDUType::Reconnect => 9,
            SNPDUType::PageRequest => 10,
            SNPDUType::PageResponse => 10,
            SNPDUType::NotSupported => 11,
            SNPDUType::DataPriority => 12,
            SNPDUType::Modify => 13,
            SNPDUType::Reserved => 14,
        }
    }
}


