use std::cell::RefCell;
use std::collections::{HashMap};
use std::rc::Rc;
use tetra_config::bluestation::SharedConfig;
use tetra_core::{multiframes, SsiType, TdmaTime, TetraAddress};
use crate::umac::subcomp::bs_sched::NUM_TIMESLOTS;
use crate::umac::subcomp::slot_locator::slot_locator::SlotLocator;

/// T.209 Inactivity time-out on traffic channel
pub const INACTIVITY_TIMEOUT_SLOTS: i32 = multiframes!(18);

/// Tracks which channels MSs are listening to.
/// Updates the state of records based on events from various places in the stack.
#[derive(Clone)]
pub struct ChannelTrackingSlotLocator {

    /// Mapping of ISSI -> Channel
    entries: Rc<RefCell<HashMap<u32, Channel>>>,

    config: SharedConfig
}

/// The possible channels for an MS
#[derive(Clone)]
pub enum Channel {
    MCCH,
    Assigned {
        slot: u8,
        assigned_at: TdmaTime,
        last_activity: TdmaTime,
    },
    Unknown,
}


#[derive(Clone)]
pub enum ChannelEvent {

    /// MS was just allocated to an assigned channel
    Allocated(Channel),

    /// Activity just observed on a channel
    Activity(Channel),
    Released,

}

impl SlotLocator for ChannelTrackingSlotLocator {
    fn get_slots_for_address(&self, dltime: TdmaTime, address: TetraAddress) -> [bool; NUM_TIMESLOTS] {
        self.get_slots_for_address(dltime, address)
    }
}

impl ChannelTrackingSlotLocator {

    pub fn new(shared_config: SharedConfig) -> Self {
        Self {
            entries: Rc::new(RefCell::new(HashMap::new())),
            config: shared_config
        }
    }

    pub fn config(&self) -> &SharedConfig {
        &self.config
    }

    /// Based on the latest known information, return the expected downlink slot(s) for an address,
    /// which may be either an individual or group.
    ///
    /// Returns a [bool; NUM_TIMESLOTS] indicating slots on which the destination may be reachable.
    /// Signalling should be sent on all indicated slots and no others.
    pub fn get_slots_for_address(&self, dltime: TdmaTime, address: TetraAddress) -> [bool; NUM_TIMESLOTS] {

        // If it's a group address, we need to find the attached members and return an intersection
        // of all of their slots
        match address.ssi_type {

            // For individual-destination signalling...
            SsiType::Issi => {

                // Look up the entry
                let entries = self.entries.borrow();
                let entry = entries.get(&address.ssi).unwrap_or(&Channel::Unknown);

                match entry {

                    // MS was last seen on the MCCH, or is on an unknown channel (so MCCH assumed)
                    Channel::MCCH | Channel::Unknown => {
                        [true, false, false, false]
                    },

                    // MS was last seen assigned to a channel
                    Channel::Assigned { last_activity, slot, ..} => {
                        // If the timeout has expired, include the MCCH too
                        let mut slots = [false; NUM_TIMESLOTS];
                        if dltime.diff(*last_activity) > INACTIVITY_TIMEOUT_SLOTS {
                            // MCCH + assigned slot
                            slots[0] = true;
                            slots[*slot as usize - 1] = true;
                            slots
                        } else {
                            // Only assigned slot
                            slots[*slot as usize - 1] = true;
                            slots
                        }
                    },
                }

            }

            // For group-destination signalling...
            SsiType::Gssi => {

                // Find the attachments for the group
                let attached_issis = self.config.state_read().subscribers.get_group_members(address.ssi);

                attached_issis
                    .into_iter()
                    // Recurse for each attached ISSI, finding that individual's reachable slots
                    .map(|issi| self.get_slots_for_address(dltime, TetraAddress::issi(issi)))
                    // Flatten the results into a single array of booleans, where each slot is reachable if any member is reachable on that slot
                    .fold([false; NUM_TIMESLOTS], |mut acc, slots| {
                        for (i, &slot) in slots.iter().enumerate() {
                            acc[i] |= slot;
                        }
                        acc
                    })
            }
            _ => panic!("Not a valid address type for which to find downlink slots")
        }
    }

    /// Handle an event that will (potentially) update one or more MS' locations
    pub fn handle(&mut self, address: TetraAddress, event: ChannelEvent) {
        match address.ssi_type {

            // For individual updates...
            SsiType::Issi => {

                match event {

                    ChannelEvent::Allocated(to_channel) => {
                        self.entries.borrow_mut().insert(address.ssi, to_channel);
                    },

                    ChannelEvent::Activity(on_channel) => {
                        self.entries.borrow_mut().insert(address.ssi, on_channel);
                    },

                    ChannelEvent::Released => {
                        self.entries.borrow_mut().insert(address.ssi, Channel::MCCH);
                    },

                }

            }

            // For groups, update records for all currently-attached ISSIs
            SsiType::Gssi => {

                let attached_issis = self.config.state_read().subscribers.get_group_members(address.ssi);

                for attached_issi in attached_issis {
                    self.handle(TetraAddress::issi(attached_issi), event.clone());
                }

            }

            _ => panic!("Updating channel tracking for non-GSSI/ISSI address not supported")
        }
    }
}
