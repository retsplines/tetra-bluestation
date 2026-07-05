use tetra_core::{TdmaTime, TetraAddress};
use crate::umac::subcomp::bs_sched::NUM_TIMESLOTS;
use crate::umac::subcomp::slot_locator::slot_locator::SlotLocator;

pub struct SimpleSlotLocator { }

impl SlotLocator for SimpleSlotLocator {
    fn get_slots_for_address(&self, _dltime: TdmaTime, _address: TetraAddress) -> [bool; NUM_TIMESLOTS] {
        // Always MCCH
        [true, false, false, false]
    }
}

impl SimpleSlotLocator {
    pub fn new() -> Self {
        Self { }
    }
}