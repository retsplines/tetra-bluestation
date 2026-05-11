use std::collections::{HashMap, HashSet};

use tetra_config::bluestation::SharedConfig;
use tetra_core::{BitBuffer, Direction, Sap, SsiType, TdmaTime, TetraAddress, tetra_entities::TetraEntity, unimplemented_log};
use tetra_core::{Layer2Service, TimeslotOwner, TxReporter, TxState};
use tetra_pdus::cmce::enums::disconnect_cause::DisconnectCause;
use tetra_pdus::cmce::{
    enums::{
        call_timeout::CallTimeout, call_timeout_setup_phase::CallTimeoutSetupPhase, cmce_pdu_type_ul::CmcePduTypeUl,
        transmission_grant::TransmissionGrant,
    },
    fields::basic_service_information::BasicServiceInformation,
    pdus::{
        d_call_proceeding::DCallProceeding, d_connect::DConnect, d_release::DRelease, d_setup::DSetup, d_tx_ceased::DTxCeased,
        d_tx_granted::DTxGranted, u_disconnect::UDisconnect, u_release::URelease, u_setup::USetup, u_tx_ceased::UTxCeased,
        u_tx_demand::UTxDemand,
    },
    structs::cmce_circuit::CmceCircuit,
};
use tetra_pdus::cmce::pdus::d_alert::DAlert;
use tetra_pdus::cmce::pdus::d_connect_acknowledge::DConnectAcknowledge;
use tetra_pdus::cmce::pdus::u_alert::UAlert;
use tetra_pdus::cmce::pdus::u_connect::UConnect;
use tetra_saps::{
    SapMsg, SapMsgInner,
    control::{
        brew::{BrewSubscriberAction, MmSubscriberUpdate},
        call_control::{CallControl, Circuit},
        enums::{circuit_mode_type::CircuitModeType, communication_type::CommunicationType},
    },
    lcmc::{
        LcmcMleUnitdataReq,
        enums::{alloc_type::ChanAllocType, ul_dl_assignment::UlDlAssignment},
        fields::chan_alloc_req::CmceChanAllocReq,
    },
};

use crate::net_brew;
use crate::{
    MessageQueue,
    cmce::components::circuit_mgr::{CircuitMgr, CircuitMgrCmd},
};

/// Clause 11 Call Control CMCE sub-entity
pub struct CcBsSubentity {
    config: SharedConfig,
    dltime: TdmaTime,
    /// Cached D-SETUP PDUs for late-entry re-sends: call_id -> (D-SETUP PDU, dest address, tx reporter)
    cached_setups: HashMap<u16, (DSetup, TetraAddress, Option<TxReporter>)>,
    circuits: CircuitMgr,
    /// Active group calls: call_id -> call info
    calls: HashMap<u16, Call>,
    /// Registered subscriber groups (ISSI -> set of GSSIs)
    subscriber_groups: HashMap<u32, HashSet<u32>>,
    /// Listener counts per GSSI
    group_listeners: HashMap<u32, usize>,
}

/// Origin of a group call
#[derive(Clone)]
enum CallOrigin {
    /// Local MS-initiated call, needs MLE routing for individual addressing
    Local {
        caller_addr: TetraAddress, // For D-CALL-PROCEEDING, D-CONNECT routing
    },
    /// Network-initiated call from TetraPack/Brew
    Network {
        brew_uuid: uuid::Uuid, // For Brew tracking
    },
}

/// The state of a call.
/// Helps track the progress of hook-signalled calls.
/// Direct-signalled calls jump straight to Connected.
#[derive(Clone, PartialEq, Eq)]
enum CallState {

    /// U-SETUP has been received
    Requested,

    /// D-SETUP & D-CALL PROCEEDING have been sent
    SetupSent,

    /// U-ALERT has been received
    Ringing,

    /// U-CONNECT has been received
    Answered,

    /// D-CONNECT ACKNOWLEDGE has been sent
    Connected
}

/// Tracks an active group call (local or network-initiated)
#[derive(Clone)]
struct Call {

    /// The state of this call
    state: CallState,

    /// The origin of the call (either local MS, or network)
    origin: CallOrigin,

    /// The destination address for this call, also identifies the type of call (P2P vs group)
    dest_addr: TetraAddress,

    /// The current speaker's ISSI
    speaker_issi: u32,

    /// The timeslot allocated to this call (1-3)
    ts: u8,

    /// The usage number allocated to this call (4-63)
    usage: u8,

    /// True if someone is currently transmitting
    tx_active: bool,

    /// When PTT was released (for hangtime). None if transmitting.
    hangtime_start: Option<TdmaTime>,

    /// Brew session UUID — set when a network speaker is active on this call,
    /// regardless of call origin. Cleared when the network speaker ends.
    brew_uuid: Option<uuid::Uuid>,
}

impl CcBsSubentity {
    pub fn new(config: SharedConfig) -> Self {
        CcBsSubentity {
            config,
            dltime: TdmaTime::default(),
            cached_setups: HashMap::new(),
            circuits: CircuitMgr::new(),
            calls: HashMap::new(),
            subscriber_groups: HashMap::new(),
            group_listeners: HashMap::new(),
        }
    }

    pub fn set_config(&mut self, config: SharedConfig) {
        self.config = config;
    }

    fn build_d_setup_prim(pdu: &DSetup, usage: u8, ts: u8, ul_dl: UlDlAssignment) -> (BitBuffer, CmceChanAllocReq) {
        let mut sdu = BitBuffer::new_autoexpand(80);
        pdu.to_bitbuf(&mut sdu).expect("Failed to serialize DSetup");
        sdu.seek(0);
        tracing::info!("-> {:?} sdu {}", pdu, sdu.dump_bin());

        // Construct ChanAlloc descriptor for the allocated timeslot
        let mut timeslots = [false; 4];
        timeslots[ts as usize - 1] = true;
        let chan_alloc = CmceChanAllocReq {
            usage: Some(usage),
            alloc_type: ChanAllocType::Replace,
            carrier: None,
            timeslots,
            ul_dl_assigned: ul_dl,
        };
        (sdu, chan_alloc)
    }

    fn build_sapmsg(
        sdu: BitBuffer,
        chan_alloc: Option<CmceChanAllocReq>,
        address: TetraAddress,
        layer2service: Layer2Service,
        reporter: Option<TxReporter>,
    ) -> SapMsg {
        // Construct prim
        SapMsg {
            sap: Sap::LcmcSap,
            src: TetraEntity::Cmce,
            dest: TetraEntity::Mle,
            msg: SapMsgInner::LcmcMleUnitdataReq(LcmcMleUnitdataReq {
                sdu,
                handle: 0,
                endpoint_id: 0,
                link_id: 0,
                layer2service,
                pdu_prio: 0,
                layer2_qos: 0,
                stealing_permission: false,
                stealing_repeats_flag: false,
                chan_alloc,
                main_address: address,
                tx_reporter: reporter,
            }),
        }
    }

    fn build_sapmsg_stealing(sdu: BitBuffer, address: TetraAddress, ts: u8) -> SapMsg {
        // For FACCH stealing on traffic channel, must specify target timeslot
        let mut timeslots = [false; 4];
        timeslots[(ts - 1) as usize] = true;
        let chan_alloc = CmceChanAllocReq {
            usage: None,
            carrier: None,
            timeslots,
            alloc_type: ChanAllocType::Replace,
            ul_dl_assigned: UlDlAssignment::Both,
        };

        SapMsg {
            sap: Sap::LcmcSap,
            src: TetraEntity::Cmce,
            dest: TetraEntity::Mle,
            msg: SapMsgInner::LcmcMleUnitdataReq(LcmcMleUnitdataReq {
                sdu,
                handle: 0,
                endpoint_id: 0,
                link_id: 0,
                layer2service: Layer2Service::Unacknowledged, // TODO FIXME check if indeed only unacked over STCH
                pdu_prio: 0,
                layer2_qos: 0,
                stealing_permission: true,
                stealing_repeats_flag: false,
                chan_alloc: Some(chan_alloc),
                main_address: address,
                tx_reporter: None,
            }),
        }
    }

    fn build_d_release_from_d_setup(d_setup_pdu: &DSetup, disconnect_cause: DisconnectCause) -> BitBuffer {
        let pdu = DRelease {
            call_identifier: d_setup_pdu.call_identifier,
            disconnect_cause,
            notification_indicator: None,
            facility: None,
            proprietary: None,
        };

        let mut sdu = BitBuffer::new_autoexpand(32);
        pdu.to_bitbuf(&mut sdu).expect("Failed to serialize DRelease");
        sdu.seek(0);
        tracing::info!("-> {:?} sdu {}", pdu, sdu.dump_bin());

        sdu
    }

    /// Checks if there are any listeners for the given GSSI (i.e. any subscribers affiliated with the group)
    fn has_listener(&self, gssi: u32) -> bool {
        self.group_listeners.get(&gssi).copied().unwrap_or(0) > 0
    }

    /// Checks if a subscriber is available (I.e. currently camped)
    fn subscriber_available(&self, issi: u32) -> bool {
        self.config.state_read().subscribers.is_registered(issi)
    }

    fn inc_group_listener(&mut self, gssi: u32) {
        let entry = self.group_listeners.entry(gssi).or_insert(0);
        *entry += 1;
    }

    fn dec_group_listener(&mut self, gssi: u32) {
        if let Some(entry) = self.group_listeners.get_mut(&gssi) {
            if *entry <= 1 {
                self.group_listeners.remove(&gssi);
            } else {
                *entry -= 1;
            }
        }
    }

    fn drop_group_calls_if_unlistened(&mut self, queue: &mut MessageQueue, gssi: u32) {
        if self.has_listener(gssi) {
            return;
        }

        let to_drop: Vec<(u16, CallOrigin)> = self
            .calls
            .iter()
            .filter(|(_, call)| call.dest_addr.ssi == gssi && call.dest_addr.ssi_type == SsiType::Gssi)
            .map(|(call_id, call)| (*call_id, call.origin.clone()))
            .collect();

        for (call_id, origin) in to_drop {
            tracing::info!("CMCE: dropping call_id={} gssi={} (no listeners)", call_id, gssi);
            if let CallOrigin::Network { brew_uuid } = origin {
                if net_brew::is_brew_gssi_routable(&self.config, gssi) {
                    queue.push_back(SapMsg {
                        sap: Sap::Control,
                        src: TetraEntity::Cmce,
                        dest: TetraEntity::Brew,
                        msg: SapMsgInner::CmceCallControl(CallControl::NetworkCallEnd { brew_uuid }),
                    });
                };
            };
            self.release_call(queue, call_id, DisconnectCause::SwmiRequestedDisconnection);
        }
    }

    pub fn handle_subscriber_update(&mut self, queue: &mut MessageQueue, update: MmSubscriberUpdate) {
        let issi = update.issi;
        let groups = update.groups;

        match update.action {
            BrewSubscriberAction::Register => {
                let known = self.subscriber_groups.contains_key(&issi);
                self.subscriber_groups.entry(issi).or_insert_with(HashSet::new);
                tracing::info!("CMCE: subscriber register issi={} known={}", issi, known);
            }
            BrewSubscriberAction::Deregister => {
                if let Some(existing) = self.subscriber_groups.remove(&issi) {
                    for gssi in existing {
                        self.dec_group_listener(gssi);
                        self.drop_group_calls_if_unlistened(queue, gssi);
                    }
                }
                tracing::info!("CMCE: subscriber deregister issi={}", issi);
            }
            BrewSubscriberAction::Affiliate => {
                let mut new_groups = Vec::new();
                {
                    let entry = self.subscriber_groups.entry(issi).or_insert_with(HashSet::new);
                    for gssi in groups {
                        if entry.insert(gssi) {
                            new_groups.push(gssi);
                        }
                    }
                }
                for gssi in &new_groups {
                    self.inc_group_listener(*gssi);
                }

                if new_groups.is_empty() {
                    tracing::debug!("CMCE: affiliate ignored (no new groups) issi={}", issi);
                } else {
                    tracing::info!("CMCE: subscriber affiliate issi={} groups={:?}", issi, new_groups);
                }
            }
            BrewSubscriberAction::Deaffiliate => {
                let mut removed_groups = Vec::new();
                let mut known_issi = false;
                if let Some(entry) = self.subscriber_groups.get_mut(&issi) {
                    known_issi = true;
                    for gssi in groups {
                        if entry.remove(&gssi) {
                            removed_groups.push(gssi);
                        }
                    }
                } else {
                    removed_groups = groups;
                }
                if known_issi {
                    for gssi in &removed_groups {
                        self.dec_group_listener(*gssi);
                    }
                }

                if removed_groups.is_empty() {
                    tracing::debug!("CMCE: deaffiliate ignored (no matching groups) issi={}", issi);
                } else {
                    tracing::info!("CMCE: subscriber deaffiliate issi={} groups={:?}", issi, removed_groups);
                    for gssi in &removed_groups {
                        self.drop_group_calls_if_unlistened(queue, *gssi);
                    }
                }
            }
        }
    }

    fn send_d_call_proceeding(&mut self, queue: &mut MessageQueue, message: &SapMsg, pdu_request: &USetup, call_id: u16) {
        tracing::trace!("send_d_call_proceeding");

        let SapMsgInner::LcmcMleUnitdataInd(prim) = &message.msg else {
            panic!()
        };

        let pdu_response = DCallProceeding {
            call_identifier: call_id,
            call_time_out_set_up_phase: CallTimeoutSetupPhase::T10s,
            hook_method_selection: pdu_request.hook_method_selection,
            simplex_duplex_selection: pdu_request.simplex_duplex_selection,
            basic_service_information: None, // Only needed if different from requested
            call_status: None,
            notification_indicator: None,
            facility: None,
            proprietary: None,
        };

        let mut sdu = BitBuffer::new_autoexpand(25);
        pdu_response.to_bitbuf(&mut sdu).expect("Failed to serialize DCallProceeding");
        sdu.seek(0);
        tracing::info!("-> {:?} sdu {}", pdu_response, sdu.dump_bin());

        let msg = SapMsg {
            sap: Sap::LcmcSap,
            src: TetraEntity::Cmce,
            dest: TetraEntity::Mle,
            msg: SapMsgInner::LcmcMleUnitdataReq(LcmcMleUnitdataReq {
                sdu,
                handle: prim.handle,
                endpoint_id: prim.endpoint_id,
                link_id: prim.link_id,
                layer2service: Layer2Service::Acknowledged,
                pdu_prio: 0,
                layer2_qos: 0,
                stealing_permission: false,
                stealing_repeats_flag: false,

                chan_alloc: None,
                main_address: prim.received_tetra_address,
                tx_reporter: None,
            }),
        };
        queue.push_back(msg);
    }

    fn signal_umac_circuit_open(queue: &mut MessageQueue, call: &CmceCircuit) {
        let circuit = Circuit {
            direction: call.direction,
            ts: call.ts,
            usage: call.usage,
            circuit_mode: call.circuit_mode,
            speech_service: call.speech_service,
            etee_encrypted: call.etee_encrypted,
        };
        let cmd = SapMsg {
            sap: Sap::Control,
            src: TetraEntity::Cmce,
            dest: TetraEntity::Umac,
            msg: SapMsgInner::CmceCallControl(CallControl::Open(circuit)),
        };
        queue.push_back(cmd);
    }

    fn signal_umac_circuit_close(queue: &mut MessageQueue, circuit: CmceCircuit) {
        let cmd = SapMsg {
            sap: Sap::Control,
            src: TetraEntity::Cmce,
            dest: TetraEntity::Umac,
            msg: SapMsgInner::CmceCallControl(CallControl::Close(circuit.direction, circuit.ts)),
        };
        queue.push_back(cmd);
    }

    fn do_group_call_setup(&mut self, call: Call) {

    }

    fn do_individual_call_setup(&mut self, call: Call) {

    }

    ///
    /// Handle the receipt of a U-ALERT PDU from an MS.
    /// This indicates that the called MS is alerting (ringing).
    ///
    /// We need to tell the calling MS that the called MS is alerting.
    ///
    /// TODO: This should probably cause a call state update.
    ///
    fn rx_u_alert(&mut self, queue: &mut MessageQueue, mut message: SapMsg) {

        tracing::info!("rx_u_alert: {:?}", message);

        let SapMsgInner::LcmcMleUnitdataInd(prim) = &mut message.msg else {
            panic!()
        };

        let called_party = prim.received_tetra_address;

        let pdu = match UAlert::from_bitbuf(&mut prim.sdu) {
            Ok(pdu) => {
                tracing::debug!("<- {:?}", pdu);
                pdu
            }
            Err(e) => {
                tracing::warn!("Failed parsing U-ALERT: {:?} {}", e, prim.sdu.dump_bin());
                return;
            }
        };

        tracing::info!("Called party {} is alerting with call_id={}", called_party, pdu.call_identifier);

        // Find the call, and if valid, update state and send D-ALERT to the calling party
        let call = match self.calls.get_mut(&pdu.call_identifier) {
            Some(call) => call,
            None => {
                tracing::warn!("Received U-ALERT for unknown call_id={}", pdu.call_identifier);
                return;
            }
        };

        call.state = CallState::Ringing;

        // Generate a D-ALERT to notify the calling party that the called party is alerting
        let d_alert = DAlert {
            call_identifier: pdu.call_identifier,
            call_time_out_set_up_phase: CallTimeoutSetupPhase::T30s,
            reserved: false,
            simplex_duplex_selection: pdu.simplex_duplex_selection,
            call_queued: false,
            basic_service_information: None,
            notification_indicator: None,
            facility: None,
            proprietary: None,
        };

        let mut alert_sdu = BitBuffer::new_autoexpand(30);
        d_alert.to_bitbuf(&mut alert_sdu).expect("Failed to serialize DAlert");
        alert_sdu.seek(0);

        // This message will include the channel allocation for the calling MS
        let connect_msg = SapMsg {
            sap: Sap::LcmcSap,
            src: TetraEntity::Cmce,
            dest: TetraEntity::Mle,
            msg: SapMsgInner::LcmcMleUnitdataReq(LcmcMleUnitdataReq {
                sdu: alert_sdu,
                handle: 0, // TODO: Check - do we need to store/reuse this from the call?
                endpoint_id: 0, // TODO: Check - do we need to store/reuse this from the call?
                link_id: 0, // TODO: Check - do we need to store/reuse this from the call?
                layer2service: Layer2Service::Unacknowledged,
                pdu_prio: 0,
                layer2_qos: 0,
                stealing_permission: false,
                stealing_repeats_flag: false,
                chan_alloc: None,
                main_address: match call.origin {
                    CallOrigin::Local { caller_addr } => caller_addr,
                    CallOrigin::Network { .. } => todo!("Not valid for network-originated calls."),
                },
                tx_reporter: None,
            }),
        };
        queue.push_back(connect_msg);

        tracing::info!("Notified caller about alerting called party {} for call_id={}", called_party, pdu.call_identifier);
    }

    ///
    /// Handle the receipt of a U-CONNECT PDU from an MS.
    ///
    /// "This PDU shall be the acknowledgement to the SwMI that the called MS is ready
    /// for through-connection." (14.7.2.3)
    ///
    /// This PDU basically indicates that a hook signalling call has been "answered".
    fn rx_u_connect(&mut self, queue: &mut MessageQueue, mut message: SapMsg) {

        tracing::info!("rx_u_connect: {:?}", message);

        let SapMsgInner::LcmcMleUnitdataInd(prim) = &mut message.msg else {
            panic!()
        };

        let called_party = prim.received_tetra_address;

        let pdu = match UConnect::from_bitbuf(&mut prim.sdu) {
            Ok(pdu) => {
                tracing::debug!("<- {:?}", pdu);
                pdu
            }
            Err(e) => {
                tracing::warn!("Failed parsing U-CONNECT: {:?} {}", e, prim.sdu.dump_bin());
                return;
            }
        };

        // Find the Call ID, and if valid, transition the call to Connected
        if let Some(call) = self.calls.get_mut(&pdu.call_identifier) {

            // TODO: More validation here
            call.state = CallState::Answered;

            let mut timeslots = [false; 4];
            timeslots[call.ts as usize - 1] = true;

            // Send D-CONNECT to the calling party
            let d_connect = DConnect {
                call_identifier: pdu.call_identifier,
                call_time_out: CallTimeout::T5m,
                hook_method_selection: pdu.hook_method_selection,
                simplex_duplex_selection: pdu.simplex_duplex_selection,
                transmission_grant: TransmissionGrant::NotGranted,
                transmission_request_permission: true,
                call_ownership: true,
                call_priority: None,
                basic_service_information: None,
                temporary_address: None,
                notification_indicator: None,
                facility: None,
                proprietary: None,
            };

            let mut connect_sdu = BitBuffer::new_autoexpand(30);
            d_connect.to_bitbuf(&mut connect_sdu).expect("Failed to serialize DConnect");
            connect_sdu.seek(0);
            tracing::info!("-> {:?} sdu {}", d_connect, connect_sdu.dump_bin());

            // This message will include the channel allocation for the calling MS
            let connect_msg = SapMsg {
                sap: Sap::LcmcSap,
                src: TetraEntity::Cmce,
                dest: TetraEntity::Mle,
                msg: SapMsgInner::LcmcMleUnitdataReq(LcmcMleUnitdataReq {
                    sdu: connect_sdu,
                    handle: 0, // TODO: Check - do we need to store/reuse this from the call?
                    endpoint_id: 0, // TODO: Check - do we need to store/reuse this from the call?
                    link_id: 0, // TODO: Check - do we need to store/reuse this from the call?
                    layer2service: Layer2Service::Unacknowledged,
                    pdu_prio: 0,
                    layer2_qos: 0,
                    stealing_permission: false,
                    stealing_repeats_flag: false,
                    chan_alloc: Some(CmceChanAllocReq {
                        usage: Some(call.usage),
                        alloc_type: ChanAllocType::Replace,
                        carrier: None,
                        timeslots,
                        ul_dl_assigned: UlDlAssignment::Both,
                    }),
                    main_address: match call.origin {
                        CallOrigin::Local { caller_addr } => caller_addr,
                        CallOrigin::Network { .. } => todo!("Not valid for network-originated calls."),
                    },
                    tx_reporter: None,
                }),
            };
            queue.push_back(connect_msg);

            // These are the handle, link ID and Endpoint ID of the *called* MS (that just sent the D-CONNECT)
            let ul_handle = prim.handle;
            let ul_link_id = prim.link_id;
            let ul_endpoint_id = prim.endpoint_id;
        }

    }

    /// Check for any calls in Answered state that need to be sent a D-CONNECT ACKNOWLEDGE to transition to Connected.
    fn check_answered_calls(&mut self, queue: &mut MessageQueue) {

        let answered: Vec<(&u16, &mut Call)> = self
            .calls
            .iter_mut()
            .filter(|(_, call)| call.state == CallState::Answered)
            .collect();

        // For each answered call, send D-CONNECT ACKNOWLEDGE and transition to Connected
        for (call_id, call) in answered {

            Self::send_d_connect_acknowledge(queue, *call_id, call);

            // Update the call state
            call.state = CallState::Connected;

        }
    }

    fn send_d_connect_acknowledge(queue: &mut MessageQueue, call_id: u16, call: &Call) {

        let mut timeslots = [false; 4];
        timeslots[call.ts as usize - 1] = true;

        let d_connect_ack = DConnectAcknowledge {
            call_identifier: call_id,
            call_time_out: CallTimeout::T5m,
            transmission_grant: TransmissionGrant::Granted,
            transmission_request_permission: true,
            notification_indicator: None,
            facility: None,
            proprietary: None,
        };

        let mut connect_ack_sdu = BitBuffer::new_autoexpand(30);
        d_connect_ack.to_bitbuf(&mut connect_ack_sdu).expect("Failed to serialize DConnectAcknowledge");
        connect_ack_sdu.seek(0);
        tracing::info!("-> {:?} sdu {}", d_connect_ack, connect_ack_sdu.dump_bin());

        // This message will include the channel allocation for the called MS
        let connect_ack_msg = SapMsg {
            sap: Sap::LcmcSap,
            src: TetraEntity::Cmce,
            dest: TetraEntity::Mle,
            msg: SapMsgInner::LcmcMleUnitdataReq(LcmcMleUnitdataReq {
                sdu: connect_ack_sdu,
                handle: 0, // TODO: Should be copied from the original U-CONNECT?
                endpoint_id: 0, // TODO: Should be copied from the original U-CONNECT?
                link_id: 0, // TODO: Should be copied from the original U-CONNECT?
                layer2service: Layer2Service::Unacknowledged,
                pdu_prio: 0,
                layer2_qos: 0,
                stealing_permission: false,
                stealing_repeats_flag: false,
                chan_alloc: Some(CmceChanAllocReq {
                    usage: Some(call.usage),
                    alloc_type: ChanAllocType::Replace,
                    carrier: None,
                    timeslots,
                    ul_dl_assigned: UlDlAssignment::Both,
                }),
                main_address: call.dest_addr,
                tx_reporter: None,
            }),
        };
        queue.push_back(connect_ack_msg);

        tracing::info!("Sending D-CONNECT ACKNOWLEDGE to called party {} with call_id={} ts={} usage={}", call.dest_addr, call_id, call.ts, call.usage);
    }

    /// Received a U-SETUP from an MS
    /// Decide what type of call this is (P2P vs Group), what type of signalling will be used (hook vs direct)
    fn rx_u_setup(&mut self, queue: &mut MessageQueue, mut message: SapMsg) {

        tracing::trace!("rx_u_setup: {:?}", message);

        let SapMsgInner::LcmcMleUnitdataInd(prim) = &mut message.msg else {
            panic!()
        };
        let calling_party = prim.received_tetra_address;

        let pdu = match USetup::from_bitbuf(&mut prim.sdu) {
            Ok(pdu) => {
                tracing::debug!("<- {:?}", pdu);
                pdu
            }
            Err(e) => {
                tracing::warn!("Failed parsing U-SETUP: {:?} {}", e, prim.sdu.dump_bin());
                return;
            }
        };

        // Check if we can satisfy this request
        if !Self::feature_check_u_setup(&pdu) {
            tracing::error!("Unsupported critical features in USetup");
            return;
        }

        // Get destination SSI (called party)
        let Some(dest_ssi) = pdu.called_party_ssi else {
            tracing::warn!("U-SETUP without called_party_ssi, ignoring");
            return;
        };
        let dest_ssi = dest_ssi as u32;
        let dest_addr = TetraAddress::new(dest_ssi, pdu.basic_service_information.communication_type.into());

        // Check if the destination subscriber is available for P2P, or the group has listeners for P2MP
        match dest_addr.ssi_type {
            SsiType::Issi => {
                if !self.subscriber_available(dest_ssi) {
                    tracing::info!("U-SETUP for ISSI {} which is not currently associated, ignoring", dest_ssi);
                    return;
                }
            }
            SsiType::Gssi => {
                if !self.has_listener(dest_ssi) {
                    tracing::info!("U-SETUP for GSSI {} which has no listeners, ignoring", dest_ssi);
                    return;
                }
            }
            _ => {
                tracing::warn!("U-SETUP with invalid called party SSI type, ignoring");
                return;
            }
        }

        // Allocate circuit (DL+UL for simplex call)
        // The allocation is done early, but the assignment will happen later
        let circuit = match {
            let mut state = self.config.state_write();
            self.circuits.allocate_circuit_with_allocator(
                Direction::Both,
                pdu.basic_service_information.communication_type,
                &mut state.timeslot_alloc,
                TimeslotOwner::Cmce,
            )
        } {
            Ok(circuit) => circuit.clone(),
            Err(e) => {
                tracing::error!("Failed to allocate circuit for U-SETUP: {:?}", e);
                return;
            }
        };

        // Track the active local call — caller is granted the floor, so tx_active = true
        self.calls.insert(
            circuit.call_id,
            Call {
                state: match dest_addr.ssi_type {
                    SsiType::Issi => CallState::Requested,
                    SsiType::Gssi => CallState::Connected, // Group calls jump straight to Connected state
                    _ => panic!("Invalid SSI type"),
                },
                origin: CallOrigin::Local {
                    caller_addr: calling_party,
                },
                dest_addr,
                speaker_issi: calling_party.ssi,
                ts: circuit.ts,
                usage: circuit.usage,
                tx_active: true,
                hangtime_start: None,
                brew_uuid: None,
            },
        );


        tracing::info!(
            "rx_u_setup: call from ISSI {} to {} {} → ts={} call_id={} usage={}",
            calling_party.ssi,
            dest_addr.ssi_type,
            dest_ssi,
            circuit.ts,
            circuit.call_id,
            circuit.usage
        );

        // Signal UMAC to open DL+UL circuits
        Self::signal_umac_circuit_open(queue, &circuit);

        // Build channel allocation timeslot mask for this call
        let mut timeslots = [false; 4];
        timeslots[circuit.ts as usize - 1] = true;

        // Extract UL message routing info (handle, link_id, endpoint_id) for
        // individually-addressed responses. These are needed so MLE can route
        // the response back to the correct radio via the established LLC link.
        let SapMsgInner::LcmcMleUnitdataInd(prim) = &message.msg else {
            panic!()
        };
        let ul_handle = prim.handle;
        let ul_link_id = prim.link_id;
        let ul_endpoint_id = prim.endpoint_id;

        // === 1) Send D-CALL-PROCEEDING to the calling MS (individually addressed) ===
        // This acknowledges the U-SETUP and keeps the radio from timing out.
        self.send_d_call_proceeding(queue, &message, &pdu, circuit.call_id);

        // === 2) Send D-SETUP to destination (broadcast on MCCH with channel allocation) ===
        // GrantedToOtherUser tells other group members that someone else has the floor.
        //
        // See 14.5.2.1.2 for the specific behaviour around this. Succinctly:
        // The calling MS will ignore group-addressed D-SETUPs that match the address & call ID,
        // so we don't have to worry about this being sent before D-CONNECT
        let d_setup = DSetup {
            call_identifier: circuit.call_id,
            call_time_out: CallTimeout::T5m,
            hook_method_selection: pdu.hook_method_selection,
            simplex_duplex_selection: pdu.simplex_duplex_selection,
            basic_service_information: pdu.basic_service_information.clone(),
            transmission_grant: if circuit.comm_type == CommunicationType::P2Mp {
                TransmissionGrant::GrantedToOtherUser
            } else {
                TransmissionGrant::NotGranted
            },
            transmission_request_permission: circuit.comm_type == CommunicationType::P2p,
            call_priority: pdu.call_priority,
            notification_indicator: None,
            temporary_address: None,
            calling_party_address_ssi: Some(calling_party.ssi),
            calling_party_extension: None,
            external_subscriber_number: None,
            facility: None,
            dm_ms_address: None,
            proprietary: None,
        };

        // Cache for late-entry re-sends. Receipt starts as None so the CircuitMgr-triggered
        // backup send (within D_SETUP_REPEATS frames) is not throttled by this initial send.
        // The first re-send via tick_start will create a tracked receipt.
        self.cached_setups.insert(circuit.call_id, (d_setup, dest_addr, None));
        let (d_setup_ref, _, _) = self.cached_setups.get(&circuit.call_id).unwrap();

        // At this point, if the call is either group-addressed, proceed to send D-CONNECT and transition to Connected.
        // Otherwise, keep the call in the "Ringing" state until we receive a U-CONNECT from the called party.
        if dest_addr.ssi_type == SsiType::Gssi {

            // Send the D-SETUP *with* channel allocation immediately
            let (setup_sdu, setup_chan_alloc) =
                Self::build_d_setup_prim(d_setup_ref, circuit.usage, circuit.ts, UlDlAssignment::Both);
            let setup_msg = Self::build_sapmsg(setup_sdu, Some(setup_chan_alloc), dest_addr, Layer2Service::Unacknowledged, None);
            queue.push_back(setup_msg);

            // === 3) Send D-CONNECT to the calling MS with Granted + channel allocation ===
            // This transitions the calling MS from "Call Setup" to "Active".
            // Uses the correct MLE handle (not 0) so MLE routes it properly.
            let d_connect = DConnect {
                call_identifier: circuit.call_id,
                call_time_out: CallTimeout::T5m,
                hook_method_selection: pdu.hook_method_selection,
                simplex_duplex_selection: pdu.simplex_duplex_selection,
                transmission_grant: TransmissionGrant::GrantedToOtherUser,
                transmission_request_permission: true,
                call_ownership: true, // Calling MS is the call owner (ETSI 14.8.4)
                call_priority: None,
                basic_service_information: None,
                temporary_address: None,
                notification_indicator: None,
                facility: None,
                proprietary: None,
            };

            let mut connect_sdu = BitBuffer::new_autoexpand(30);
            d_connect.to_bitbuf(&mut connect_sdu).expect("Failed to serialize DConnect");
            connect_sdu.seek(0);
            tracing::info!("-> {:?} sdu {}", d_connect, connect_sdu.dump_bin());

            // This message will include the channel allocation for the calling MS
            // This is the "Late Assignment Group Call" case in 14.5.3.1.
            let connect_msg = SapMsg {
                sap: Sap::LcmcSap,
                src: TetraEntity::Cmce,
                dest: TetraEntity::Mle,
                msg: SapMsgInner::LcmcMleUnitdataReq(LcmcMleUnitdataReq {
                    sdu: connect_sdu,
                    handle: ul_handle,
                    endpoint_id: ul_endpoint_id,
                    link_id: ul_link_id,
                    layer2service: Layer2Service::Unacknowledged,
                    pdu_prio: 0,
                    layer2_qos: 0,
                    stealing_permission: false,
                    stealing_repeats_flag: false,
                    chan_alloc: Some(CmceChanAllocReq {
                        usage: Some(circuit.usage),
                        alloc_type: ChanAllocType::Replace,
                        carrier: None,
                        timeslots,
                        ul_dl_assigned: UlDlAssignment::Both,
                    }),
                    main_address: calling_party,
                    tx_reporter: None,
                }),
            };
            queue.push_back(connect_msg);


        } else {

            tracing::info!("Sending individual-addressed D-SETUP with no chanalloc: {:?}", d_setup_ref);

            // Send the D-SETUP *with* channel allocation immediately
            // Also we won't be re-sending late-entry D-SETUPs for individual calls.
            let (setup_sdu, setup_chan_alloc) =
                Self::build_d_setup_prim(&d_setup_ref, circuit.usage, circuit.ts, UlDlAssignment::Both);
            let setup_msg = Self::build_sapmsg(setup_sdu, None, dest_addr, Layer2Service::Unacknowledged, None);
            queue.push_back(setup_msg);

            // No more activity for this call until we receive a U-CONNECT from the called party.
            // Call is now "SetupSent"
            let call = self.calls.get_mut(&circuit.call_id).unwrap();
            call.state = CallState::SetupSent;
        }

        // Notify Brew entity about this local call if Brew is loaded and the SSI is cleared for Brew
        // It can then forward to TetraPack if the group is subscribed
        if dest_addr.ssi_type == SsiType::Gssi && net_brew::is_brew_gssi_routable(&self.config, dest_addr.ssi) {
            let msg = SapMsg {
                sap: Sap::Control,
                src: TetraEntity::Cmce,
                dest: TetraEntity::Brew,
                msg: SapMsgInner::CmceCallControl(CallControl::FloorGranted {
                    call_id: circuit.call_id,
                    source_issi: calling_party.ssi,
                    dest_gssi:  dest_addr.ssi,
                    ts: circuit.ts,
                }),
            };
            queue.push_back(msg);
        }
    }

    pub fn route_xx_deliver(&mut self, _queue: &mut MessageQueue, mut message: SapMsg) {
        tracing::trace!("route_xx_deliver");

        let SapMsgInner::LcmcMleUnitdataInd(prim) = &mut message.msg else {
            panic!();
        };
        let Some(bits) = prim.sdu.peek_bits(5) else {
            tracing::warn!("insufficient bits: {}", prim.sdu.dump_bin());
            return;
        };
        let Ok(pdu_type) = CmcePduTypeUl::try_from(bits) else {
            tracing::warn!("invalid pdu type: {} in {}", bits, prim.sdu.dump_bin());
            return;
        };

        // TODO FIXME: Besides these PDUs, we can also receive several signals (BUSY ind, CLOSE ind, etc)
        match pdu_type {
            CmcePduTypeUl::USetup => self.rx_u_setup(_queue, message),
            CmcePduTypeUl::UTxCeased => self.rx_u_tx_ceased(_queue, message),
            CmcePduTypeUl::UTxDemand => self.rx_u_tx_demand(_queue, message),
            CmcePduTypeUl::URelease => self.rx_u_release(_queue, message),
            CmcePduTypeUl::UDisconnect => self.rx_u_disconnect(_queue, message),
            CmcePduTypeUl::UConnect => self.rx_u_connect(_queue, message),
            CmcePduTypeUl::UAlert => self.rx_u_alert(_queue, message),
            CmcePduTypeUl::UInfo
            | CmcePduTypeUl::UStatus
            | CmcePduTypeUl::UCallRestore => {
                unimplemented_log!("{}", pdu_type);
            }
            _ => {
                panic!();
            }
        }
    }

    pub fn tick_start(&mut self, queue: &mut MessageQueue, dltime: TdmaTime) {

        self.dltime = dltime;

        // Check for any answered calls that need D-CONNECT ACKNOWLEDGE sending to the called party
        (self).check_answered_calls(queue);

        // Check hangtime expiry for active local calls
        self.check_hangtime_expiry(queue);

        if let Some(tasks) = self.circuits.tick_start(dltime) {
            for task in tasks {
                match task {

                    CircuitMgrCmd::SendDSetup(call_id, usage, ts) => {
                        // Skip late-entry D-SETUP during hangtime. The traffic channel is still
                        // allocated and sending D-SETUP with NotGranted can prevent floor requests.
                        if let Some(active) = self.calls.get(&call_id) {
                            if active.hangtime_start.is_some() {
                                continue;
                            }
                        }

                        // Get our cached D-SETUP, build a prim and send it down the stack
                        let Some((pdu, dest_addr, receipt)) = self.cached_setups.get_mut(&call_id) else {
                            tracing::error!("No cached D-SETUP for call id {}", call_id);
                            continue;
                        };

                        // Throttle: if the previous D-SETUP hasn't reached a final state yet
                        // (still queued in UMAC), skip this re-send to avoid flooding the MCCH.
                        if let Some(r) = receipt.as_ref() {
                            if !r.is_in_final_state() {
                                tracing::trace!(
                                    "Suppressing D-SETUP re-send for call_id={} (previous still {:?})",
                                    call_id,
                                    r.get_state()
                                );
                                continue;
                            }
                            if r.get_state() == TxState::Discarded {
                                tracing::debug!("Previous D-SETUP for call_id={} was discarded by UMAC, retrying", call_id);
                            }
                        }

                        // Update transmission_grant based on current call state:
                        // During hangtime (nobody transmitting), use NotGranted;
                        // during active TX, use GrantedToOtherUser.
                        if let Some(active) = self.calls.get(&call_id) {
                            pdu.transmission_grant = if active.tx_active {
                                TransmissionGrant::GrantedToOtherUser
                            } else {
                                TransmissionGrant::NotGranted
                            };
                        }
                        let dest_addr = *dest_addr;
                        let (sdu, chan_alloc) = Self::build_d_setup_prim(pdu, usage, ts, UlDlAssignment::Both);

                        // Create a fresh txreporter for this re-send
                        let reporter = TxReporter::new_unacked();

                        // Cache the setup in cached_setups with the reporter so we can check its state on the next tick and throttle if it's still pending in UMAC
                        *receipt = Some(reporter.clone());

                        let prim = Self::build_sapmsg(sdu, Some(chan_alloc), dest_addr, Layer2Service::Unacknowledged, Some(reporter));
                        queue.push_back(prim);
                    }

                    CircuitMgrCmd::SendClose(call_id, circuit) => {
                        tracing::warn!("need to send CLOSE for call id {}", call_id);
                        let ts = circuit.ts;
                        // Get our cached D-SETUP, build D-RELEASE and send
                        if let Some((pdu, dest_addr, _)) = self.cached_setups.get(&call_id) {
                            let dest_addr = *dest_addr;
                            let sdu = Self::build_d_release_from_d_setup(pdu, DisconnectCause::ExpiryOfTimer);
                            let prim = Self::build_sapmsg(sdu, None, dest_addr, Layer2Service::Unacknowledged, None);
                            queue.push_back(prim);
                        } else {
                            tracing::error!("No cached D-SETUP for call id {}", call_id);
                        }

                        // Clean up call state
                        self.cached_setups.remove(&call_id);
                        self.calls.remove(&call_id);

                        // Signal UMAC to release the circuit
                        Self::signal_umac_circuit_close(queue, circuit);
                        self.release_timeslot(ts);
                    }
                }
            }
        }
    }

    /// Check if any active calls in hangtime have expired, and if so, release them
    fn check_hangtime_expiry(&mut self, queue: &mut MessageQueue) {
        // Hangtime: 5 multiframes = ~5 seconds
        const HANGTIME_FRAMES: i32 = 5 * 18 * 4;

        let expired: Vec<u16> = self
            .calls
            .iter()
            // Only group calls
            .filter(|(_, call)| matches!(call.dest_addr.ssi_type, SsiType::Gssi))
            .filter_map(|(&call_id, call)| {
                if let Some(hangtime_start) = call.hangtime_start {
                    if hangtime_start.age(self.dltime) > HANGTIME_FRAMES {
                        return Some(call_id);
                    }
                }
                None
            })
            .collect();

        for call_id in expired {
            tracing::info!("Hangtime expired for call_id={}, releasing", call_id);
            self.release_call(queue, call_id, DisconnectCause::ExpiryOfTimer);
        }
    }

    fn release_timeslot(&mut self, ts: u8) {
        let mut state = self.config.state_write();
        if let Err(err) = state.timeslot_alloc.release(TimeslotOwner::Cmce, ts) {
            tracing::warn!("CcBsSubentity: failed to release timeslot ts={} err={:?}", ts, err);
        }
    }

    /// Release a call: send D-RELEASE, close circuits, clean up state
    fn release_call(&mut self, queue: &mut MessageQueue, call_id: u16, disconnect_cause: DisconnectCause) {
        let Some((pdu, dest_addr, _)) = self.cached_setups.get(&call_id) else {
            tracing::error!("No cached D-SETUP for call_id={}", call_id);
            return;
        };
        let dest_addr = *dest_addr;

        // Send D-RELEASE to group
        let sdu = Self::build_d_release_from_d_setup(pdu, disconnect_cause);
        let prim = if let Some(ts) = self.calls.get(&call_id).map(|c| c.ts) {
            Self::build_sapmsg_stealing(sdu, dest_addr, ts)
        } else {
            tracing::warn!(
                "release_call: no active call state for call_id={}, sending D-RELEASE on MCCH",
                call_id
            );
            Self::build_sapmsg(sdu, None, dest_addr, Layer2Service::Unacknowledged, None)
        };
        queue.push_back(prim);

        // Close the circuit in CircuitMgr and notify Brew
        if let Some(call) = self.calls.get(&call_id) {
            let ts = call.ts;
            let dest_ssi = call.dest_addr;
            let is_local = matches!(call.origin, CallOrigin::Local { .. });

            if let Ok(circuit) = self.circuits.close_circuit(Direction::Both, ts) {
                Self::signal_umac_circuit_close(queue, circuit);
            }

            // Ensure UMAC clears hangtime even if the CMCE circuit was already closed above.
            queue.push_back(SapMsg {
                sap: Sap::Control,
                src: TetraEntity::Cmce,
                dest: TetraEntity::Umac,
                msg: SapMsgInner::CmceCallControl(CallControl::CallEnded { call_id, ts }),
            });

            self.release_timeslot(ts);

            // Notify Brew only for local calls on SSIs that are cleared for Brew
            if dest_ssi.ssi_type == SsiType::Gssi && net_brew::is_brew_gssi_routable(&self.config, dest_addr.ssi) {
                if is_local {
                    let notify = SapMsg {
                        sap: Sap::Control,
                        src: TetraEntity::Cmce,
                        dest: TetraEntity::Brew,
                        msg: SapMsgInner::CmceCallControl(CallControl::CallEnded { call_id, ts }),
                    };
                    queue.push_back(notify);
                }
            }
        }

        // Clean up
        self.cached_setups.remove(&call_id);
        self.calls.remove(&call_id);
    }

    fn feature_check_u_setup(pdu: &USetup) -> bool {
        let mut supported = true;

        if !(pdu.area_selection == 0 || pdu.area_selection == 1) {
            unimplemented_log!("Area selection not supported: {}", pdu.area_selection);
            supported = false;
        };
        if pdu.simplex_duplex_selection != false {
            unimplemented_log!("Only simplex calls supported: {}", pdu.simplex_duplex_selection);
            supported = false;
        };
        // if pdu.basic_service_information != 0xFC {
        //     // TODO FIXME implement parsing
        //     tracing::error!("Basic service information not supported: {}", pdu.basic_service_information);
        //     return;
        // };
        // request_to_transmit_send_data can be false for speech group calls — the MS
        // implicitly requests to transmit by initiating the call. No action needed.
        if pdu.clir_control != 0 {
            unimplemented_log!("clir_control not supported: {}", pdu.clir_control);
        };
        if pdu.called_party_ssi.is_none() || pdu.called_party_short_number_address.is_some() || pdu.called_party_extension.is_some() {
            unimplemented_log!("we only support ssi-based calling");
        };
        // Then, we warn about some other unhandled/unsupported fields
        if let Some(v) = &pdu.external_subscriber_number {
            unimplemented_log!("external_subscriber_number not supported: {:?}", v);
        };
        if let Some(v) = &pdu.facility {
            unimplemented_log!("facility not supported: {:?}", v);
        };
        if let Some(v) = &pdu.dm_ms_address {
            unimplemented_log!("dm_ms_address not supported: {:?}", v);
        };
        if let Some(v) = &pdu.proprietary {
            unimplemented_log!("proprietary not supported: {:?}", v);
        };

        supported
    }

    /// Handle U-TX CEASED: radio released PTT
    /// Response: send D-TX CEASED via FACCH to all group members, enter hangtime
    fn rx_u_tx_ceased(&mut self, queue: &mut MessageQueue, mut message: SapMsg) {
        let SapMsgInner::LcmcMleUnitdataInd(prim) = &mut message.msg else {
            panic!()
        };

        let pdu = match UTxCeased::from_bitbuf(&mut prim.sdu) {
            Ok(pdu) => {
                tracing::debug!("<- {:?}", pdu);
                pdu
            }
            Err(e) => {
                tracing::warn!("Failed parsing U-TX CEASED: {:?}", e);
                return;
            }
        };

        let call_id = pdu.call_identifier;

        // Look up the active call
        let Some(call) = self.calls.get_mut(&call_id) else {
            tracing::warn!("U-TX CEASED for unknown call_id={}", call_id);
            return;
        };

        // Check if already in hangtime - ignore duplicate U-TX CEASED to avoid resetting timer
        if !call.tx_active && call.hangtime_start.is_some() {
            tracing::debug!("U-TX CEASED: already in hangtime for call_id={}, ignoring duplicate", call_id);
            return;
        }

        tracing::info!("U-TX CEASED: PTT released on call_id={}, entering hangtime", call_id);

        let ts = call.ts;
        let dest_ssi = call.dest_addr;
        call.tx_active = false;
        call.hangtime_start = Some(self.dltime);

        // Get dest address from cached setup
        let Some((_, dest_addr, _)) = self.cached_setups.get(&call_id) else {
            tracing::error!("No cached D-SETUP for call_id={}", call_id);
            return;
        };
        let dest_addr = *dest_addr;

        // Send D-TX CEASED via FACCH (stealing) to all group members
        let d_tx_ceased = DTxCeased {
            call_identifier: call_id,
            transmission_request_permission: false, // ETSI 14.8.43: 0 = allowed to request transmission
            notification_indicator: None,
            facility: None,
            dm_ms_address: None,
            proprietary: None,
        };

        let mut sdu = BitBuffer::new_autoexpand(25);
        d_tx_ceased.to_bitbuf(&mut sdu).expect("Failed to serialize DTxCeased");
        sdu.seek(0);
        tracing::info!("-> {:?} sdu {}", d_tx_ceased, sdu.dump_bin());

        // Send via FACCH (stealing channel) so radios on the traffic channel hear the beep
        let msg = Self::build_sapmsg_stealing(sdu, dest_addr, ts);
        queue.push_back(msg);

        // Notify UMAC to enter hangtime signalling mode on this traffic timeslot.
        queue.push_back(SapMsg {
            sap: Sap::Control,
            src: TetraEntity::Cmce,
            dest: TetraEntity::Umac,
            msg: SapMsgInner::CmceCallControl(CallControl::FloorReleased { call_id, ts }),
        });

        // Notify Brew to stop forwarding audio, if this SSI is cleared for Br
        if dest_addr.ssi_type == SsiType::Gssi && net_brew::is_brew_gssi_routable(&self.config, dest_ssi.ssi) {
            queue.push_back(SapMsg {
                sap: Sap::Control,
                src: TetraEntity::Cmce,
                dest: TetraEntity::Brew,
                msg: SapMsgInner::CmceCallControl(CallControl::FloorReleased { call_id, ts }),
            });
        }
    }

    /// Handle U-TX DEMAND: another radio requests floor during hangtime
    /// Response: send D-TX GRANTED via FACCH, resume voice path
    fn rx_u_tx_demand(&mut self, queue: &mut MessageQueue, mut message: SapMsg) {
        let SapMsgInner::LcmcMleUnitdataInd(prim) = &mut message.msg else {
            panic!()
        };
        let requesting_party = prim.received_tetra_address;

        let pdu = match UTxDemand::from_bitbuf(&mut prim.sdu) {
            Ok(pdu) => {
                tracing::debug!("<- {:?}", pdu);
                pdu
            }
            Err(e) => {
                tracing::warn!("Failed parsing U-TX DEMAND: {:?}", e);
                return;
            }
        };

        let call_id = pdu.call_identifier;

        let Some(call) = self.calls.get_mut(&call_id) else {
            tracing::warn!("U-TX DEMAND for unknown call_id={}", call_id);
            return;
        };

        tracing::info!("U-TX DEMAND: ISSI {} requests floor on call_id={}", requesting_party.ssi, call_id);

        // ETSI 14.5.2.2.1 b): if another MS is already transmitting, the SwMI should
        // normally wait for that party to finish before granting. Reject the request.
        if call.tx_active {
            tracing::warn!(
                "U-TX DEMAND from ISSI {} rejected, ISSI {} already transmitting on call_id={}",
                requesting_party.ssi,
                call.speaker_issi,
                call_id
            );
            return;
        }

        // Grant the floor to the requesting MS
        let ts = call.ts;
        call.tx_active = true;
        call.hangtime_start = None;
        call.speaker_issi = requesting_party.ssi;

        // Update caller_addr for local calls
        if let CallOrigin::Local { caller_addr } = &mut call.origin {
            *caller_addr = requesting_party;
        }

        let Some((_, dest_addr, _)) = self.cached_setups.get(&call_id) else {
            tracing::error!("No cached D-SETUP for call_id={}", call_id);
            return;
        };
        let dest_addr = *dest_addr;

        // ETSI 14.5.2.2.1 b): Send individual D-TX GRANTED (Granted) to requesting MS FIRST
        let d_tx_granted_individual = DTxGranted {
            call_identifier: call_id,
            transmission_grant: TransmissionGrant::Granted.into_raw() as u8,
            transmission_request_permission: false,
            encryption_control: false,
            reserved: false,
            notification_indicator: None,
            transmitting_party_type_identifier: Some(1), // SSI
            transmitting_party_address_ssi: Some(requesting_party.ssi as u64),
            transmitting_party_extension: None,
            external_subscriber_number: None,
            facility: None,
            dm_ms_address: None,
            proprietary: None,
        };

        let mut sdu = BitBuffer::new_autoexpand(50);
        d_tx_granted_individual.to_bitbuf(&mut sdu).expect("Failed to serialize DTxGranted");
        sdu.seek(0);
        tracing::info!("-> {:?} sdu {}", d_tx_granted_individual, sdu.dump_bin());

        let requesting_addr = TetraAddress::new(requesting_party.ssi, SsiType::Issi);
        let msg = Self::build_sapmsg_stealing(sdu, requesting_addr, ts);
        queue.push_back(msg);

        // ETSI 14.5.2.2.1 b): Send group D-TX GRANTED (GrantedToOtherUser) to GSSI
        self.send_d_tx_granted_facch(queue, call_id, requesting_party.ssi, dest_addr.ssi, ts);

        // Notify UMAC to resume traffic mode (exit hangtime) for this timeslot.
        queue.push_back(SapMsg {
            sap: Sap::Control,
            src: TetraEntity::Cmce,
            dest: TetraEntity::Umac,
            msg: SapMsgInner::CmceCallControl(CallControl::FloorGranted {
                call_id,
                source_issi: requesting_party.ssi,
                dest_gssi: dest_addr.ssi,
                ts,
            }),
        });

        // Notify Brew of speaker change (local MS taking floor)
        if net_brew::is_brew_gssi_routable(&self.config, dest_addr.ssi) {
            let Some(call) = self.calls.get(&call_id) else {
                return;
            };
            queue.push_back(SapMsg {
                sap: Sap::Control,
                src: TetraEntity::Cmce,
                dest: TetraEntity::Brew,
                msg: SapMsgInner::CmceCallControl(CallControl::FloorGranted {
                    call_id,
                    source_issi: requesting_party.ssi,
                    dest_gssi: dest_addr.ssi,
                    ts: call.ts,
                }),
            });
        }
    }

    /// Handle U-RELEASE: radio explicitly releases the call
    fn rx_u_release(&mut self, queue: &mut MessageQueue, mut message: SapMsg) {
        let SapMsgInner::LcmcMleUnitdataInd(prim) = &mut message.msg else {
            panic!()
        };

        let pdu = match URelease::from_bitbuf(&mut prim.sdu) {
            Ok(pdu) => {
                tracing::debug!("<- {:?}", pdu);
                pdu
            }
            Err(e) => {
                tracing::warn!("Failed parsing U-RELEASE: {:?}", e);
                return;
            }
        };

        let call_id = pdu.call_identifier;
        tracing::info!("U-RELEASE: call_id={} cause={}", call_id, pdu.disconnect_cause);
        self.release_call(queue, call_id, DisconnectCause::UserRequestedDisconnection);
    }

    /// Handle U-DISCONNECT: MS requests call disconnection (ETSI 14.5.2.3.1)
    /// Call owner → release entire group call with D-RELEASE (cause=1)
    /// Non-call owner → reject with D-RELEASE cause=8 individually addressed to sender
    fn rx_u_disconnect(&mut self, queue: &mut MessageQueue, mut message: SapMsg) {
        let SapMsgInner::LcmcMleUnitdataInd(prim) = &mut message.msg else {
            panic!()
        };
        let sender = prim.received_tetra_address;
        let ul_handle = prim.handle;
        let ul_link_id = prim.link_id;
        let ul_endpoint_id = prim.endpoint_id;

        let pdu = match UDisconnect::from_bitbuf(&mut prim.sdu) {
            Ok(pdu) => {
                tracing::debug!("<- {:?}", pdu);
                pdu
            }
            Err(e) => {
                tracing::warn!("Failed parsing U-DISCONNECT: {:?}", e);
                return;
            }
        };

        let call_id = pdu.call_identifier;
        let disconnect_cause = pdu.disconnect_cause;

        let Some(call) = self.calls.get(&call_id) else {
            tracing::debug!("U-DISCONNECT for unknown call_id={} (likely duplicate)", call_id);
            return;
        };

        let is_call_owner = matches!(&call.origin, CallOrigin::Local { caller_addr } if caller_addr.ssi == sender.ssi);

        if is_call_owner {
            // Call owner: tear down the entire group call
            tracing::info!("U-DISCONNECT: call owner ISSI {} disconnecting call_id={}", sender.ssi, call_id);
            self.release_call(queue, call_id, DisconnectCause::UserRequestedDisconnection);
        } else {
            // Non-call owner: reject with D-RELEASE cause=8 ("Requested service not available")
            // individually addressed back to the sender. The group call continues.
            tracing::info!(
                "U-DISCONNECT: non-call-owner ISSI {} rejected for call_id={} cause={}",
                sender.ssi,
                call_id,
                disconnect_cause
            );

            let d_release = DRelease {
                call_identifier: call_id,
                disconnect_cause: DisconnectCause::RequestedServiceNotAvailable,
                notification_indicator: None,
                facility: None,
                proprietary: None,
            };

            let mut sdu = BitBuffer::new_autoexpand(32);
            d_release.to_bitbuf(&mut sdu).expect("Failed to serialize DRelease");
            sdu.seek(0);
            tracing::info!("-> {:?} sdu {}", d_release, sdu.dump_bin());

            let sender_addr = TetraAddress::new(sender.ssi, SsiType::Issi);
            let msg = SapMsg {
                sap: Sap::LcmcSap,
                src: TetraEntity::Cmce,
                dest: TetraEntity::Mle,
                msg: SapMsgInner::LcmcMleUnitdataReq(LcmcMleUnitdataReq {
                    sdu,
                    handle: ul_handle,
                    endpoint_id: ul_endpoint_id,
                    link_id: ul_link_id,
                    layer2service: Layer2Service::Unacknowledged,
                    pdu_prio: 0,
                    layer2_qos: 0,
                    stealing_permission: false,
                    stealing_repeats_flag: false,
                    chan_alloc: None,
                    main_address: sender_addr,
                    tx_reporter: None,
                }),
            };
            queue.push_back(msg);
        }
    }

    /// Handle incoming CallControl messages from Brew
    pub fn rx_call_control(&mut self, queue: &mut MessageQueue, message: SapMsg) {
        let SapMsgInner::CmceCallControl(call_control) = message.msg else {
            panic!("Expected CmceCallControl message");
        };

        match call_control {
            CallControl::NetworkCallStart {
                brew_uuid,
                source_issi,
                dest_gssi,
                priority,
            } => {
                self.rx_network_call_start(queue, brew_uuid, source_issi, dest_gssi, priority);
            }
            CallControl::NetworkCallEnd { brew_uuid } => {
                self.rx_network_call_end(queue, brew_uuid);
            }
            CallControl::UlInactivityTimeout { ts } => {
                self.handle_ul_inactivity_timeout(queue, ts);
            }
            _ => {
                tracing::warn!("Unexpected CallControl message: {:?}", call_control);
            }
        }
    }

    /// Handle network-initiated group call start
    fn rx_network_call_start(&mut self, queue: &mut MessageQueue, brew_uuid: uuid::Uuid, source_issi: u32, dest_gssi: u32, _priority: u8) {
        assert!(net_brew::is_brew_gssi_routable(&self.config, dest_gssi));

        if !self.has_listener(dest_gssi) {
            tracing::info!(
                "CMCE: ignoring network call start uuid={} gssi={} (no listeners)",
                brew_uuid,
                dest_gssi
            );
            self.drop_group_calls_if_unlistened(queue, dest_gssi);

            // We already checked this is cleared for brew
            queue.push_back(SapMsg {
                sap: Sap::Control,
                src: TetraEntity::Cmce,
                dest: TetraEntity::Brew,
                msg: SapMsgInner::CmceCallControl(CallControl::NetworkCallEnd { brew_uuid }),
            });
            return;
        }

        // Check if there is an active call for this GSSI (speaker change scenario)
        if let Some((call_id, call)) = self.calls.iter_mut().find(|(_, c)| c.dest_addr.ssi_type == SsiType::Gssi && c.dest_addr.ssi == dest_gssi ) {
            // Reject speaker change if a local MS is already transmitting
            if call.tx_active {
                tracing::warn!(
                    "CMCE: network speaker change rejected, ISSI {} already transmitting on gssi={}",
                    call.speaker_issi,
                    dest_gssi
                );
                queue.push_back(SapMsg {
                    sap: Sap::Control,
                    src: TetraEntity::Cmce,
                    dest: TetraEntity::Brew,
                    msg: SapMsgInner::CmceCallControl(CallControl::NetworkCallEnd { brew_uuid }),
                });
                return;
            }

            // Speaker change during hangtime
            tracing::info!(
                "CMCE: network call speaker change gssi={} new_speaker={} (was {})",
                dest_gssi,
                source_issi,
                call.speaker_issi
            );

            call.speaker_issi = source_issi;
            call.tx_active = true;
            call.hangtime_start = None;
            call.brew_uuid = Some(brew_uuid);

            if let CallOrigin::Network { brew_uuid: old_uuid } = call.origin {
                // Update UUID if different (shouldn't happen but handle it)
                if old_uuid != brew_uuid {
                    tracing::warn!("CMCE: brew_uuid changed during speaker change");
                    call.origin = CallOrigin::Network { brew_uuid };
                }
            }

            // Extract values before mutable borrow ends
            let call_id_val = *call_id;
            let ts = call.ts;
            let usage = call.usage;

            // End the mutable borrow
            let _ = call;

            // Send D-TX GRANTED via FACCH to notify radios of new speaker
            self.send_d_tx_granted_facch(queue, call_id_val, source_issi, dest_gssi, ts);

            // Notify UMAC to resume traffic mode (exit hangtime) for this timeslot.
            queue.push_back(SapMsg {
                sap: Sap::Control,
                src: TetraEntity::Cmce,
                dest: TetraEntity::Umac,
                msg: SapMsgInner::CmceCallControl(CallControl::FloorGranted {
                    call_id: call_id_val,
                    source_issi,
                    dest_gssi,
                    ts,
                }),
            });

            // Respond to Brew with existing call resources, we already ensured it is cleared for brew
            queue.push_back(SapMsg {
                sap: Sap::Control,
                src: TetraEntity::Cmce,
                dest: TetraEntity::Brew,
                msg: SapMsgInner::CmceCallControl(CallControl::NetworkCallReady {
                    brew_uuid,
                    call_id: call_id_val,
                    ts,
                    usage,
                }),
            });
            return;
        }

        // New network call - allocate circuit
        let circuit = match {
            let mut state = self.config.state_write();
            self.circuits.allocate_circuit_with_allocator(
                Direction::Both,
                CommunicationType::P2Mp,
                &mut state.timeslot_alloc,
                TimeslotOwner::Cmce,
            )
        } {
            Ok(c) => c.clone(),
            Err(err) => {
                tracing::warn!("CMCE: failed to allocate circuit for network call: {:?}", err);
                return;
            }
        };

        let call_id = circuit.call_id;
        let ts = circuit.ts;
        let usage = circuit.usage;

        tracing::info!(
            "CMCE: starting NEW network call brew_uuid={} gssi={} speaker={} ts={} call_id={}",
            brew_uuid,
            dest_gssi,
            source_issi,
            ts,
            call_id
        );

        // Signal UMAC to open DL and UL circuits
        Self::signal_umac_circuit_open(queue, &circuit);

        tracing::debug!(
            "CMCE: sending D-SETUP for NEW call call_id={} gssi={} (network-initiated)",
            call_id,
            dest_gssi
        );

        // Send D-SETUP to group (broadcast on MCCH)
        let dest_addr = TetraAddress::new(dest_gssi, SsiType::Gssi);
        let d_setup = DSetup {
            call_identifier: call_id,
            call_time_out: CallTimeout::T5m,
            hook_method_selection: false,
            simplex_duplex_selection: false, // Simplex
            basic_service_information: BasicServiceInformation {
                circuit_mode_type: CircuitModeType::TchS,
                encryption_flag: false,
                communication_type: CommunicationType::P2Mp,
                slots_per_frame: None,
                speech_service: Some(0),
            },
            transmission_grant: TransmissionGrant::GrantedToOtherUser,
            transmission_request_permission: false,
            call_priority: 0,
            notification_indicator: None,
            temporary_address: None,
            calling_party_address_ssi: Some(source_issi),
            calling_party_extension: None,
            external_subscriber_number: None,
            facility: None,
            dm_ms_address: None,
            proprietary: None,
        };

        // Cache for late-entry re-sends. Receipt starts as None so the CircuitMgr-triggered
        // backup send (within D_SETUP_REPEATS frames) is not throttled by this initial send.
        // The first re-send via tick_start will create a tracked receipt.
        self.cached_setups.insert(call_id, (d_setup, dest_addr, None));
        let (d_setup_ref, _, _) = self.cached_setups.get(&call_id).unwrap();

        let (setup_sdu, setup_chan_alloc) = Self::build_d_setup_prim(d_setup_ref, usage, ts, UlDlAssignment::Both);
        let setup_msg = Self::build_sapmsg(setup_sdu, Some(setup_chan_alloc), dest_addr, Layer2Service::Unacknowledged, None);
        queue.push_back(setup_msg);

        // Send D-CONNECT to group
        let d_connect = DConnect {
            call_identifier: call_id,
            call_time_out: CallTimeout::T5m,
            hook_method_selection: false,
            simplex_duplex_selection: false, // Simplex
            transmission_grant: TransmissionGrant::GrantedToOtherUser,
            transmission_request_permission: false,
            call_ownership: false,
            call_priority: None,
            basic_service_information: None,
            temporary_address: None,
            notification_indicator: None,
            facility: None,
            proprietary: None,
        };

        let mut connect_sdu = BitBuffer::new_autoexpand(30);
        d_connect.to_bitbuf(&mut connect_sdu).expect("Failed to serialize DConnect");
        connect_sdu.seek(0);

        let connect_msg = SapMsg {
            sap: Sap::LcmcSap,
            src: TetraEntity::Cmce,
            dest: TetraEntity::Mle,
            msg: SapMsgInner::LcmcMleUnitdataReq(LcmcMleUnitdataReq {
                sdu: connect_sdu,
                handle: 0, // Broadcast to group, no specific handle
                endpoint_id: 0,
                link_id: 0,
                layer2service: Layer2Service::Unacknowledged,
                pdu_prio: 0,
                layer2_qos: 0,
                stealing_permission: false,
                stealing_repeats_flag: false,
                chan_alloc: None, // Already sent in D-SETUP
                main_address: dest_addr,
                tx_reporter: None,
            }),
        };
        queue.push_back(connect_msg);

        // Track the active call
        self.calls.insert(
            call_id,
            Call {
                state: CallState::Connected,
                origin: CallOrigin::Network { brew_uuid },
                dest_addr,
                speaker_issi: source_issi,
                ts,
                usage,
                tx_active: true,
                hangtime_start: None,
                brew_uuid: Some(brew_uuid),
            },
        );

        // Respond to Brew with allocated resources, we already ensured it is cleared for brew
        queue.push_back(SapMsg {
            sap: Sap::Control,
            src: TetraEntity::Cmce,
            dest: TetraEntity::Brew,
            msg: SapMsgInner::CmceCallControl(CallControl::NetworkCallReady {
                brew_uuid,
                call_id,
                ts,
                usage,
            }),
        });
    }

    /// Handle network call end request
    fn rx_network_call_end(&mut self, queue: &mut MessageQueue, brew_uuid: uuid::Uuid) {
        // Find the call by brew_uuid field (works for both Local and Network origin calls)
        let Some((call_id, call)) = self
            .calls
            .iter()
            .find(|(_, c)| c.brew_uuid == Some(brew_uuid))
            .map(|(id, c)| (*id, c.clone()))
        else {
            tracing::debug!("CMCE: network call end for unknown brew_uuid={}", brew_uuid);
            return;
        };

        tracing::info!(
            "CMCE: network call ended brew_uuid={} call_id={} addr={}",
            brew_uuid,
            call_id,
            call.dest_addr
        );

        // If currently transmitting, enter hangtime instead of immediate release
        let tx_active = call.tx_active;
        let ts = call.ts;

        if tx_active {
            if let Some(active_call) = self.calls.get_mut(&call_id) {
                active_call.tx_active = false;
                active_call.hangtime_start = Some(self.dltime);
                active_call.brew_uuid = None;
            }
            // Send D-TX CEASED via FACCH
            self.send_d_tx_ceased_facch(queue, call_id, call.dest_addr.ssi, ts);

            // Notify UMAC to enter hangtime signalling mode on this traffic timeslot.
            queue.push_back(SapMsg {
                sap: Sap::Control,
                src: TetraEntity::Cmce,
                dest: TetraEntity::Umac,
                msg: SapMsgInner::CmceCallControl(CallControl::FloorReleased { call_id, ts }),
            });
        } else {
            // Already in hangtime or idle, release immediately
            self.release_call(queue, call_id, DisconnectCause::SwmiRequestedDisconnection);
        }
    }

    /// Send D-TX GRANTED via FACCH stealing
    fn send_d_tx_granted_facch(&mut self, queue: &mut MessageQueue, call_id: u16, source_issi: u32, dest_gssi: u32, ts: u8) {
        let pdu = DTxGranted {
            call_identifier: call_id,
            transmission_grant: TransmissionGrant::GrantedToOtherUser.into_raw() as u8,
            transmission_request_permission: false,
            encryption_control: false,
            reserved: false,
            notification_indicator: None,
            transmitting_party_type_identifier: Some(1), // SSI
            transmitting_party_address_ssi: Some(source_issi as u64),
            transmitting_party_extension: None,
            external_subscriber_number: None,
            facility: None,
            dm_ms_address: None,
            proprietary: None,
        };

        let mut sdu = BitBuffer::new_autoexpand(30);
        pdu.to_bitbuf(&mut sdu).expect("Failed to serialize DTxGranted");
        sdu.seek(0);
        tracing::info!("-> FACCH {:?} sdu {}", pdu, sdu.dump_bin());

        let dest_addr = TetraAddress::new(dest_gssi, SsiType::Gssi);
        let msg = Self::build_sapmsg_stealing(sdu, dest_addr, ts);
        queue.push_back(msg);
    }

    /// Handle UL inactivity timeout from UMAC: a radio disappeared mid-transmission.
    /// Treat identically to rx_u_tx_ceased — force TX ceased, enter hangtime.
    fn handle_ul_inactivity_timeout(&mut self, queue: &mut MessageQueue, ts: u8) {

        // Find the active call on this timeslot with tx_active == true
        let call_entry = self
            .calls
            .iter()
            .find(|(_, call)| call.ts == ts && call.tx_active)
            .map(|(id, _)| *id);

        let Some(call_id) = call_entry else {
            tracing::debug!("UL inactivity timeout on ts={} but no active transmitting call found", ts);
            return;
        };

        let call = self.calls.get_mut(&call_id).unwrap();
        tracing::warn!("UL inactivity timeout on ts={}, forcing TX ceased for call_id={}", ts, call_id);

        let dest_addr = call.dest_addr;
        call.tx_active = false;
        call.hangtime_start = Some(self.dltime);

        // Send D-TX CEASED via FACCH to all group members
        self.send_d_tx_ceased_facch(queue, call_id, dest_addr.ssi, ts);

        // Notify UMAC to enter hangtime signalling mode
        queue.push_back(SapMsg {
            sap: Sap::Control,
            src: TetraEntity::Cmce,
            dest: TetraEntity::Umac,
            msg: SapMsgInner::CmceCallControl(CallControl::FloorReleased { call_id, ts }),
        });

        // Notify Brew to stop forwarding audio
        if dest_addr.ssi_type == SsiType::Gssi && net_brew::is_brew_gssi_routable(&self.config, dest_addr.ssi) {
            queue.push_back(SapMsg {
                sap: Sap::Control,
                src: TetraEntity::Cmce,
                dest: TetraEntity::Brew,
                msg: SapMsgInner::CmceCallControl(CallControl::FloorReleased { call_id, ts }),
            });
        }
    }

    /// Send D-TX CEASED via FACCH stealing
    fn send_d_tx_ceased_facch(&mut self, queue: &mut MessageQueue, call_id: u16, dest_gssi: u32, ts: u8) {
        let pdu = DTxCeased {
            call_identifier: call_id,
            transmission_request_permission: false, // ETSI 14.8.43: 0 = allowed to request transmission
            notification_indicator: None,
            facility: None,
            dm_ms_address: None,
            proprietary: None,
        };

        let mut sdu = BitBuffer::new_autoexpand(30);
        pdu.to_bitbuf(&mut sdu).expect("Failed to serialize DTxCeased");
        sdu.seek(0);
        tracing::info!("-> FACCH {:?} sdu {}", pdu, sdu.dump_bin());

        let dest_addr = TetraAddress::new(dest_gssi, SsiType::Gssi);
        let msg = Self::build_sapmsg_stealing(sdu, dest_addr, ts);
        queue.push_back(msg);
    }
}
