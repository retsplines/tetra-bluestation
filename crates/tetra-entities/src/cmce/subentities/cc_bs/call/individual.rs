use tetra_core::TetraAddress;
use crate::cmce::subentities::cc_bs::call::event::{CallEvent, ProcessesCallEvents};

pub enum IndividualCallState {

    /// The call has been requested but setup signalling has not yet been sent
    Pending,

    /// Setup signalling has been sent.
    /// The called party has not yet indicated that they are alerting.
    SetupSent,

    /// The called party is alerting but has not yet accepted the call
    /// On entering this state, CC shall inform the caller that the called party is alerting.
    CalledPartyAlerting,

    /// The call is in hangtime (there is no active speaker but the call has not yet been terminated)
    Hangtime,
}

pub struct IndividualCall {

    /// The caller for this call
    caller: TetraAddress,

    /// The called-party of this call
    callee: TetraAddress,

    /// The current state of the call
    state: IndividualCallState

}

impl ProcessesCallEvents for IndividualCall {
    fn process_call_event(&mut self, event: CallEvent) {
        todo!()
    }
}