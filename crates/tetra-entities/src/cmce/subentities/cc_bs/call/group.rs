use tetra_core::TetraAddress;
use crate::cmce::subentities::cc_bs::call::event::{CallEvent, ProcessesCallEvents};

enum GroupCallState {

    /// The call has been requested but setup signalling has not yet been sent
    Pending,

    /// The call is active
    Active,

    /// The call is in hangtime (there is no active speaker but the call has not yet been terminated)
    Hangtime,

}

pub struct GroupCall {

    /// The caller for this call
    caller: TetraAddress,

    /// The called-party of this call
    callee: TetraAddress,

    /// The current state of the call
    state: GroupCallState

}

impl ProcessesCallEvents for GroupCall {
    fn process_call_event(&mut self, event: CallEvent) {
        todo!()
    }
}
