use tetra_core::{unimplemented_log, TdmaTime};
use tetra_pdus::cmce::enums::cmce_pdu_type_ul::CmcePduTypeUl;
use tetra_saps::{SapMsg, SapMsgInner};
use crate::cmce::subentities::cc_bs::call::Call;
use crate::MessageQueue;

mod call;

struct CcBsSubentity {

    /// Calls currently being tracked by the CMCE CC sub-entity
    calls: Vec<Call>

}

impl CcBsSubentity {

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


    /// Tick start handler
    pub fn tick_start(&mut self, queue: &mut MessageQueue, dltime: TdmaTime) {
        todo!()
    }




}
