use tetra_core::{Direction, TdmaTime};
use tetra_saps::{
    control::enums::{circuit_mode_type::CircuitModeType, communication_type::CommunicationType},
    lcmc::CallId,
};

/// A circuit managed by the CMCE.
#[derive(Debug, Clone)]
pub struct Circuit {

    /// Time when this circuit was created
    pub ts_created: TdmaTime,

    /// Direction
    pub direction: Direction,

    /// Timeslot in which this circuit exists
    pub ts: u8,

    /// Call ID as allocated by CMCE
    pub call_id: CallId,

    /// Usage number, between 4 and 63
    /// This is the identity of the circuit as advertised in ACCESS-ASSIGN PDUs
    pub usage: u8,

    /// Traffic channel type
    pub circuit_mode: CircuitModeType,

    /// 2 opt, 00 = TETRA encoded speech, 1|2 = reserved, 3 = proprietary
    pub speech_service: Option<u8>,

}
