use std::collections::VecDeque;
use tetra_core::{TdmaTime, TetraAddress};
use crate::cmce::subentities::cc_bs::call::event::{CallEvent, ProcessesCallEvents};
use crate::cmce::subentities::cc_bs::call::group::GroupCall;
use crate::cmce::subentities::cc_bs::call::individual::IndividualCall;
use crate::cmce::subentities::cc_bs::call::signal::CallSignal;

mod individual;
mod group;
mod event;
mod signal;

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

/// The possible types of call
pub enum CallType {
    Group(GroupCall),
    Individual(IndividualCall)
}

struct LateEntryState {

    /// The last time the call was advertised for late entry
    last_advertised: TdmaTime

}

/// Owns information about the current speaker in a call
struct SpeakerState {

    /// The current speaker for this call, if any
    current_speaker: Option<CallEndpoint>,

    /// The last time the speaker changed
    last_change_time: TdmaTime

}

/// An active call being tracked by the CC sub-entity
pub struct Call {

    /// The caller for this call
    caller: CallEndpoint,

    /// The called-party of this call
    callee: CallEndpoint,

    /// The current speaker state, if relevant
    speaker_state: Option<SpeakerState>,

    /// The usage marker for this call, which covers all associated channels
    usage_marker: u8,

    /// The type of call
    call_type: CallType,

    /// Late Entry mechanism control & state
    late_entry: Option<LateEntryState>,

    /// A queue of signals emitted by this call that have not yet been processed by the CC sub-entity
    pending_signals: VecDeque<CallSignal>,

}

impl ProcessesCallEvents for Call {

    fn process_call_event(&mut self, event: CallEvent) {
        // Proxy the event to the relevant call type handler
        match &mut self.call_type {
            CallType::Group(group_call) => group_call.process_call_event(event),
            CallType::Individual(individual_call) => individual_call.process_call_event(event),
        }
    }

}