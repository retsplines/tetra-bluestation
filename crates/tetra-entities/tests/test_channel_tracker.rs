mod common;

use tetra_config::bluestation::StackMode;
use tetra_core::{debug, SsiType, TdmaTime, TetraAddress};
use tetra_entities::umac::subcomp::slot_locator::channel_tracking_slot_locator::{Channel, ChannelEvent, ChannelTrackingSlotLocator, INACTIVITY_TIMEOUT_SLOTS};
use crate::common::ComponentTest;

#[test]
fn test_individual_channels_tracked_correctly() {
    debug::setup_logging_verbose();

    let test_infra = ComponentTest::new(StackMode::Bs, Some(TdmaTime::default()));
    let mut tracker = ChannelTrackingSlotLocator::new(test_infra.get_shared_config());
    let base_time = TdmaTime::default();

    // Show the tracker a channel allocation
    tracker.handle(TetraAddress::issi(1024), ChannelEvent::Allocated(Channel::Assigned {
        slot: 2,
        assigned_at: base_time,
        last_activity: base_time,
    }));

    // Check the slots immediately after, should be on slot 2
    assert_eq!(tracker.get_slots_for_address(base_time, TetraAddress::issi(1024)), [false, true, false, false]);

    // Check the slots after the expiry time,
    // Should be 1 (MCCH) + 2
    assert_eq!(tracker.get_slots_for_address(base_time.add_timeslots(INACTIVITY_TIMEOUT_SLOTS + 1), TetraAddress::issi(1024)), [true, true, false, false]);

    // Provide an update that sends the MS back to the MCCH
    tracker.handle(TetraAddress::issi(1024), ChannelEvent::Activity(Channel::MCCH));
    assert_eq!(tracker.get_slots_for_address(base_time, TetraAddress::issi(1024)), [true, false, false, false]);
}

#[test]
fn test_group_channels_tracked_correctly() {

    debug::setup_logging_verbose();

    let test_infra = ComponentTest::new(StackMode::Bs, Some(TdmaTime::default()));
    let mut tracker = ChannelTrackingSlotLocator::new(test_infra.get_shared_config());
    let base_time = TdmaTime::default();

    // Assign some ISSIs to a group
    test_infra.config.state_write().subscribers.affiliate(1024, 1);
    test_infra.config.state_write().subscribers.affiliate(1025, 1);

    // Show the tracker a channel allocation for one of the groups
    tracker.handle(TetraAddress::new(1, SsiType::Gssi), ChannelEvent::Allocated(Channel::Assigned {
        slot: 2,
        assigned_at: base_time,
        last_activity: base_time,
    }));

    // Check the slots immediately after for the affiliated ISSIs, should be on slot 2
    assert_eq!(tracker.get_slots_for_address(base_time, TetraAddress::issi(1024)), [false, true, false, false]);
    assert_eq!(tracker.get_slots_for_address(base_time, TetraAddress::issi(1025)), [false, true, false, false]);

    // And for a non-affiliated ISSI, should be on MCCH only
    assert_eq!(tracker.get_slots_for_address(base_time, TetraAddress::issi(1026)), [true, false, false, false]);

    // Advance time past the inactivity timeout, should be on MCCH + 2 for the affiliated ISSIs
    assert_eq!(tracker.get_slots_for_address(base_time.add_timeslots(INACTIVITY_TIMEOUT_SLOTS + 1), TetraAddress::issi(1024)), [true, true, false, false]);
    assert_eq!(tracker.get_slots_for_address(base_time.add_timeslots(INACTIVITY_TIMEOUT_SLOTS + 1), TetraAddress::issi(1025)), [true, true, false, false]);
}