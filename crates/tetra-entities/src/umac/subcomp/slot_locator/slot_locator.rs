use tetra_core::{TdmaTime, TetraAddress};
use crate::umac::subcomp::bs_sched::NUM_TIMESLOTS;

pub trait SlotLocator {
    fn get_slots_for_address(&self, dltime: TdmaTime, address: TetraAddress) -> [bool; NUM_TIMESLOTS];
}
