/// An event emitted by the CMCE CC sub-entity and processed by a call.
pub enum CallEvent {

    

}

pub trait ProcessesCallEvents {

    fn process_call_event(&mut self, event: CallEvent);

}
