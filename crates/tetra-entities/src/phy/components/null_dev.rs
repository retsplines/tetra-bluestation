use tetra_pdus::phy::traits::rxtx_dev::{RxSlotBits, RxTxDev, RxTxDevError, TxSlotBits};

/// A null Rx/Tx device that always successfully transmits/receives
pub struct RxTxDevNull {
}

impl RxTxDevNull {
    pub fn new() -> Self {
        RxTxDevNull {}
    }
}

impl RxTxDev for RxTxDevNull {

    fn rxtx_timeslot<'a>(
        &'a mut self,
        _tx_slot: &[TxSlotBits],
    ) -> Result<Vec<Option<RxSlotBits<'a>>>, RxTxDevError> {
        Ok(Default::default())
    }
    
}