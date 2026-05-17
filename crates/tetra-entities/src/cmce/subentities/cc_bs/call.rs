use tetra_core::{TdmaTime, TetraAddress};
use tetra_pdus::cmce::pdus::u_setup::USetup;

/// One of the endpoints involved in a call
#[derive(Clone)]
enum CallEndpoint {
    Local {
        caller_addr: TetraAddress,
    },
    Network {
        network_addr: uuid::Uuid,
    },
}

pub enum CallType {
    Group,
    IndividualSimplex,
    IndividualDuplex
}

struct LateEntryState {

    /// The last time the call was advertised for late entry
    last_advertised: TdmaTime

}

/// An active call being tracked by the CC sub-entity
pub struct Call {

    /// The caller for this call
    caller: CallEndpoint,

    /// The called-party of this call
    callee: CallEndpoint,

    /// The participant that is currently speaking
    speaker: Option<CallEndpoint>,

    /// The usage marker for this call, which covers all associated channels
    usage_marker: u8,

    /// The type of call
    call_type: CallType,

    /// Late Entry mechanism control & state
    late_entry: Option<LateEntryState>


}

impl Call {

    pub fn new_with_u_setup(u_setup: USetup) {

    }

    /// Process any on-tick actions for the call
    pub fn tick() {}


}