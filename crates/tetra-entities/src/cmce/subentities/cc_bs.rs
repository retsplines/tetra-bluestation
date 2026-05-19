use tetra_config::bluestation::SharedConfig;
use tetra_core::{unimplemented_log, Direction, PduParseErr, SsiType, TdmaTime, TetraAddress, TimeslotOwner};
use tetra_pdus::cmce::enums::cmce_pdu_type_ul::CmcePduTypeUl;
use tetra_pdus::cmce::pdus::u_setup::USetup;
use tetra_saps::{SapMsg, SapMsgInner};
use tetra_saps::control::call_control::CallControl;
use tetra_saps::control::enums::communication_type::CommunicationType;
use crate::cmce::components::circuit_manager::CircuitManager;
use crate::cmce::subentities::cc_bs::call::Call;
use crate::MessageQueue;

mod call;

struct CcBsSubentity {

    /// Calls currently being tracked by the CMCE CC sub-entity
    calls: Vec<Call>,

    /// Manager for circuits allocated by the CC sub-entity
    circuits: CircuitManager,

    /// Shared configuration
    config: SharedConfig
}

impl CcBsSubentity {

    pub fn new(config: SharedConfig) -> Self {
        CcBsSubentity {
            calls: Vec::new(),
            circuits: CircuitManager::new(),
            config
        }
    }

    fn allocate_all_circuits(&mut self, count: usize, )

    /// Handle the receipt of a U-SETUP PDU
    fn rx_u_setup(&mut self, queue: &mut MessageQueue, mut message: SapMsg) -> Result<(), String> {

        tracing::trace!("rx_u_setup: {:?}", message);
        let SapMsgInner::LcmcMleUnitdataInd(prim) = &mut message.msg else {
            panic!("unexpected message type: {:?}", message);
        };

        // Parse the PDU
        let pdu = USetup::from_bitbuf(&mut prim.sdu)
            .map_err(|e| format!("Failed to parse U-SETUP PDU: {:?}", e))?;


        // Identify the calling party & called party
        let calling_party = prim.received_tetra_address;
        let called_party = TetraAddress {
            ssi: pdu.called_party_ssi.ok_or("Missing called party SSI in U-SETUP PDU")? as u32,
            ssi_type: match pdu.basic_service_information.communication_type {
                CommunicationType::P2p => SsiType::Issi,
                CommunicationType::P2Mp | CommunicationType::P2MpAcked => SsiType::Gssi,
                _ => return Err(format!("Unsupported communication type: {:?}", pdu.basic_service_information.communication_type)),
            }
        };

        // Allocate necessary slots (one for simplex, two for duplex)
        let mut state = self.config.state_write();

        let circuits =
        let circuit = self.circuits.allocate_circuit_with_allocator(
            Direction::Both,
            pdu.basic_service_information.communication_type,
            &mut state.timeslot_alloc,
            TimeslotOwner::Cmce,
        ).map_err(|e| format!("Failed to allocate circuit: {:?}", e))?;

        tracing::info!(
            "rx_u_setup: call from ISSI {} to {} → ts={} call_id={} usage={}",
            calling_party.ssi,
            dest_gssi,
            circuit.ts,
            circuit.call_id,
            circuit.usage
        );




        Ok(())
    }

    /// Receive a primitive proxied to the CC sub-entity by the CMCE protocol controller via the XX route.
    pub fn route_xx_deliver(&mut self, _queue: &mut MessageQueue, mut message: SapMsg) {

        let SapMsgInner::LcmcMleUnitdataInd(prim) = &mut message.msg else {
            panic!("unexpected message type: {:?}", message);
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
            CmcePduTypeUl::UAlert
            | CmcePduTypeUl::UConnect
            | CmcePduTypeUl::UInfo
            | CmcePduTypeUl::UStatus
            | CmcePduTypeUl::UCallRestore => {
                unimplemented_log!("{}", pdu_type);
            }
            _ => {
                panic!();
            }
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
                todo!()
                // self.rx_network_call_start(queue, brew_uuid, source_issi, dest_gssi, priority);
            }
            CallControl::NetworkCallEnd { brew_uuid } => {
                todo!()
                // self.rx_network_call_end(queue, brew_uuid);
            }
            CallControl::UlInactivityTimeout { ts } => {
                todo!()
                // self.handle_ul_inactivity_timeout(queue, ts);
            }
            _ => {
                tracing::warn!("Unexpected CallControl message: {:?}", call_control);
            }
        }
    }


    /// Tick start handler
    pub fn tick_start(&mut self, queue: &mut MessageQueue, dltime: TdmaTime) {
        todo!()
    }




}
