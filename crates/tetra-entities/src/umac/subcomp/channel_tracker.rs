use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use tetra_core::{multiframes, TdmaTime};

/// T.209 Inactivity time-out on traffic channel
const INACTIVITY_TIMEOUT_SLOTS: i32 = multiframes!(18);

/// Tracks which channels MSs are listening to.
/// Updates the state of records based on events from various places in the stack.
#[derive(Clone)]
pub struct ChannelTracker {
    entries: Rc<RefCell<HashMap<u32, MsChannel>>>
}

/// The possible channels for an MS
pub enum MsChannel {
    MCCH,
    AssignedChannel {
        slot: u8,
        assigned_at: TdmaTime,
        last_activity: TdmaTime,
    },
    Unknown,
}

pub enum MsChannelEvent {

    /// MS was just allocated to an assigned channel
    Allocated(MsChannel),

    Activity(MsChannel),
    Released,

}


impl ChannelTracker {

    pub fn new() -> Self {
        Self {
            entries: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    /// Based on the latest known information, return the expected downlink slot(s) for an SSI
    pub fn get_slots_for_ssi(&self, dltime: TdmaTime, ssi: u32) -> Vec<u8> {

        // Look up the entry
        let entries = self.entries.borrow();
        let entry = entries.get(&ssi).unwrap_or(&MsChannel::Unknown);

        match entry {

            // MS was last seen on the MCCH, or is on an unknown channel (so MCCH assumed)
            MsChannel::MCCH | MsChannel::Unknown => {
                vec![1]
            },

            // MS was last seen assigned to a channel
            MsChannel::AssignedChannel { last_activity, slot, ..} => {
                // If the timeout has expired, include the MCCH too
                if dltime.diff(*last_activity) > INACTIVITY_TIMEOUT_SLOTS {
                    vec![1, *slot]
                } else {
                    vec![*slot]
                }
            },
        }
    }

    /// Handle an event that will (potentially) update an MS's location
    pub fn handle(&mut self, ssi: u16, event: MsChannelEvent) {
        match event {

            MsChannelEvent::Allocated(to_channel) => {
                self.entries.borrow_mut().insert(ssi, to_channel);
            },

            MsChannelEvent::Activity(on_channel) => {
                self.entries.borrow_mut().insert(ssi, on_channel);
            },

            MsChannelEvent::Released => {
                self.entries.borrow_mut().insert(ssi, MsChannel::MCCH);
            },

        }
    }
}

#[cfg(test)]
mod tests {
    use tetra_core::debug;
    use super::*;

    #[test]
    fn test_channels_tracked_correctly() {
        debug::setup_logging_verbose();

        let mut tracker = ChannelTracker::new();
        let base_time = TdmaTime::default();

        // Show the tracker a channel allocation
        tracker.handle(1024, MsChannelEvent::Allocated(MsChannel::AssignedChannel {
            slot: 2,
            assigned_at: base_time,
            last_activity: base_time,
        }));

        // Check the slots immediately after, should be on slot 2
        assert_eq!(tracker.get_slots_for_ssi(base_time, 1024), vec![2]);

        // Check the slots after the expiry time,
        // Should be 1 (MCCH) + 2
        assert_eq!(tracker.get_slots_for_ssi(base_time.add_timeslots(INACTIVITY_TIMEOUT_SLOTS + 1), 1024), vec![1, 2]);

        // Provide an update that sends the MS back to the MCCH
        tracker.handle(1024, MsChannelEvent::Activity(MsChannel::MCCH));
        assert_eq!(tracker.get_slots_for_ssi(base_time, 1024), vec![1]);
    }
}
