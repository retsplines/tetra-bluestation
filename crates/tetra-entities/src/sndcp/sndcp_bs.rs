use crate::{MessageQueue, TetraEntityTrait};
use tetra_config::bluestation::SharedConfig;
use tetra_core::tetra_entities::TetraEntity;
use tetra_core::{Sap, unimplemented_log, Direction};
use tetra_pdus::mm::enums::mm_pdu_type_ul::MmPduTypeUl;
use tetra_pdus::sndcp::enums::sn_pdu_type::SNPDUType;
use tetra_pdus::sndcp::pdus::sn_activate_pdp_context_demand::SNActivatePdpContextDemand;
use tetra_saps::{SapMsg, SapMsgInner};

pub struct Sndcp {
    // config: Option<SharedConfig>,
    config: SharedConfig,
}

impl Sndcp {
    pub fn new(config: SharedConfig) -> Self {
        Self { config }
    }
}

impl TetraEntityTrait for Sndcp {
    fn entity(&self) -> TetraEntity {
        TetraEntity::Sndcp
    }

    fn rx_prim(&mut self, _queue: &mut MessageQueue, mut message: SapMsg) {

        // Must be for the SNDCP SAP
        assert_eq!(message.sap, Sap::LtpdSap);

        // Decode the SNDCP protocol
        match &message.msg {
            SapMsgInner::LtpdMleUnitdataInd(inner) => {
                tracing::info!("Received SNDCP SDU: {:?}", inner.sdu);
            },
            _ => panic!("Unexpected message type for SNDCP: {:?}", message.msg),
        }

        tracing::debug!("rx_prim: {:?}", message);

        // There is only one SAP for SNDCP, so process PDUs here
        // Read the PDU type
        let SapMsgInner::LtpdMleUnitdataInd(prim) = &mut message.msg else {
            panic!()
        };

        let Some(bits) = prim.sdu.peek_bits(4) else {
            tracing::warn!("insufficient bits: {}", prim.sdu.dump_bin());
            return;
        };

        let Ok(pdu_type) = SNPDUType::from_raw(bits, Direction::Ul) else {
            tracing::warn!("invalid pdu type: {} in {}", bits, prim.sdu.dump_bin());
            return;
        };

        match pdu_type {
            SNPDUType::ActivatePdpContextDemand => {

                let pdu = SNActivatePdpContextDemand::from_bitbuf(
                    &mut prim.sdu, Direction::Ul
                );

                tracing::info!("Parsed SN Activate PDP Context Demand: {:?}", pdu);
            },
            _ => {
                tracing::warn!("Unsupported SNDCP PDU type: {:?} in {}", pdu_type, prim.sdu.dump_bin());
            }
        }

    }
}
