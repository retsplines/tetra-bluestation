use tetra_core::{expect_pdu_type, BitBuffer, Direction, PduParseErr, Todo};
use tetra_core::typed_pdu_fields::{delimiters, typed};
use tetra_core::typed_pdu_fields::typed::parse_type3_generic;
use crate::cmce::enums::cmce_pdu_type_ul::CmcePduTypeUl;
use crate::mm::enums::type34_elem_id_dl::MmType34ElemIdDl;
use crate::mm::fields::group_identity_downlink::GroupIdentityDownlink;
use crate::mm::pdus::d_attach_detach_group_identity::DAttachDetachGroupIdentity;
use crate::sndcp::enums::address_type_identifier_in_demand::AddressTypeIdentifierInDemand;
use crate::sndcp::enums::nsapi::NSAPI;
use crate::sndcp::enums::packet_data_ms_type::PacketDataMSType;
use crate::sndcp::enums::sn_pdu_type::SNPDUType;
use crate::sndcp::enums::sndcp_version::SNDCPVersion;
use crate::sndcp::fields::pcomp_negotiation::PCOMPNegotiation;

#[derive(Debug)]
pub struct SNActivatePdpContextDemand {

    /// Type 1, 1 bits, SNDCP Version
    pub sndcp_version: SNDCPVersion,

    /// Type 1, 1 bit, NSAPI
    pub nsapi: NSAPI,

    /// Type 1, 3 bits, Address type identifier in demand
    pub address_type_identifier: AddressTypeIdentifierInDemand,

    /// Conditional, 32 bits, IP address IPv4
    pub ip_address_ipv4: Option<u32>,

    /// Conditional, 4 bits, NSAPI
    ///
    /// Shall be conditional on the value of Address Type Identifier in Demand (ATID)
    ///  * when ATID = 5 (Primary NSAPI (secondary PDP context requested)) the information element shall be present
    ///  * for any other value of the ATID the information element shall not be present.
    ///
    /// Shall contain the NSAPI of the primary PDP context from which the requested secondary PDP context derives its PDP address.
    pub nsapi_b: Option<NSAPI>,

    /// Type 1, 4 bits, Packet Data MS Type
    pub packet_data_ms_type: PacketDataMSType,

    /// Type 1, 8 bits, PCOMP negotiation
    pub pcomp_negotiation: PCOMPNegotiation,

    /// Conditional, 8 bits, Number of Van Jacobson compression state slots
    pub num_vj_comp_state_slots: Option<u8>,

    /// Conditional, 8 bits, Number of compression state slots, TCP
    pub num_tcp_comp_state_slots: Option<u8>,

    /// Conditional, 16 bits, Number of compression state slots, non-TCP
    pub num_non_tcp_comp_state_slots: Option<u16>,

    /// Conditional, 8 bits, Maximum interval between full headers
    pub max_interval_between_full_headers: Option<u8>,

    /// Conditional, 8 bits, Largest size in octets that may be compressed
    pub largest_size_compressed: Option<u8>,

    /// Type 2, Optional, 16 bits, Access point name index
    pub access_point_name_index: Option<u16>,

    /// Type 3, Optional, Variable Length, DCOMP negotiation
    pub dcomp_negotiation: Option<Todo>,

    /// Type 3, Optional, Variable Length, Protocol configuration options
    pub protocol_configuration_options: Option<Todo>,

    /// Type 3, Optional, Variable Length, Quality of Service
    pub quality_of_service: Option<Todo>,
}

impl SNActivatePdpContextDemand {
    /// Parse from BitBuffer
    pub fn from_bitbuf(buffer: &mut BitBuffer, direction: Direction) -> Result<Self, PduParseErr> {

        // The PDU type should still be present, so we can check it here
        let pdu_type_raw = buffer.read_field(4, "pdu_type")?;
        expect_pdu_type!(pdu_type_raw, SNPDUType::ActivatePdpContextAccept)?;

        // Type 1
        let sndcp_version_raw = buffer.read_field(4, "sndcp_version")?;
        let Ok(sndcp_version) = SNDCPVersion::try_from(sndcp_version_raw) else {
            return Err(PduParseErr::InvalidValue {
                field: "sndcp_version",
                value: sndcp_version_raw,
            });
        };

        // Type 1
        let nsapi_raw = buffer.read_field(1, "nsapi")?;
        let Ok(nsapi) = NSAPI::try_from(nsapi_raw) else {
            return Err(PduParseErr::InvalidValue {
                field: "nsapi",
                value: nsapi_raw,
            });
        };

        // Type 1
        let address_type_identifier_raw = buffer.read_field(3, "address_type_identifier")?;
        let Ok(address_type_identifier) = AddressTypeIdentifierInDemand::try_from(address_type_identifier_raw) else {
            return Err(PduParseErr::InvalidValue {
                field: "address_type_identifier",
                value: address_type_identifier_raw,
            });
        };

        // Conditional on the ATID
        let ip_address_ipv4 = if address_type_identifier == AddressTypeIdentifierInDemand::IPv4Static {
            Some(buffer.read_field(32, "ip_address_ipv4")? as u32)
        } else {
            None
        };

        // Conditional on the ATID
        let nsapi_b = if address_type_identifier == AddressTypeIdentifierInDemand::PrimaryNSAPI {
            let nsapi_b_raw = buffer.read_field(4, "nsapi_b")?;
            let Ok(nsapi_b) = NSAPI::try_from(nsapi_b_raw) else {
                return Err(PduParseErr::InvalidValue {
                    field: "nsapi_b",
                    value: nsapi_b_raw,
                });
            };
            Some(nsapi_b)
        } else {
            None
        };

        // Type 1
        let packet_data_ms_type_raw = buffer.read_field(4, "packet_data_ms_type")?;
        let Ok(packet_data_ms_type) = PacketDataMSType::try_from(packet_data_ms_type_raw) else {
            return Err(PduParseErr::InvalidValue {
                field: "packet_data_ms_type",
                value: packet_data_ms_type_raw,
            });
        };

        // Type 1
        let pcomp_negotiation = PCOMPNegotiation::from_bitbuf(buffer)?;

        // Conditional on the value of bit 1 of the PCOMP negotiation field
        let num_vj_comp_state_slots = if pcomp_negotiation.vj_tcp_ip_header_compression {
            Some(buffer.read_field(8, "num_vj_comp_state_slots")? as u8)
        } else {
            None
        };

        // Conditional on the value of bit 2 of the PCOMP negotiation field
        let num_tcp_comp_state_slots = if pcomp_negotiation.ip_header_compression {
            Some(buffer.read_field(8, "num_tcp_comp_state_slots")? as u8)
        } else {
            None
        };

        // Conditional on the value of bit 2 of the PCOMP negotiation field
        let num_non_tcp_comp_state_slots = if pcomp_negotiation.ip_header_compression {
            Some(buffer.read_field(16, "num_non_tcp_comp_state_slots")? as u16)
        } else {
            None
        };

        // Conditional on the value of bit 3 of the PCOMP negotiation field
        let max_interval_between_full_headers = if pcomp_negotiation.other_header_compression {
            Some(buffer.read_field(8, "max_interval_between_full_headers")? as u8)
        } else {
            None
        };

        // Conditional on the value of bit 2 of the PCOMP negotiation field
        let largest_size_compressed = if pcomp_negotiation.ip_header_compression {
            Some(buffer.read_field(8, "largest_size_compressed")? as u8)
        } else {
            None
        };

        // Type 2
        let obit = delimiters::read_obit(buffer)?;

        // Further fields are conditional on the value of the o-bit
        let access_point_name_index = if obit {
            Some(buffer.read_field(16, "access_point_name_index")? as u16)
        } else {
            None
        };

        // TODO: There is more to parse, but let's just try to get the basic structure working first
        Ok(SNActivatePdpContextDemand {
            sndcp_version,
            nsapi,
            address_type_identifier,
            ip_address_ipv4,
            nsapi_b,
            packet_data_ms_type,
            pcomp_negotiation,
            num_vj_comp_state_slots,
            num_tcp_comp_state_slots,
            num_non_tcp_comp_state_slots,
            max_interval_between_full_headers,
            largest_size_compressed,
            access_point_name_index,
            dcomp_negotiation: None, // TODO
            protocol_configuration_options: None, // TODO
            quality_of_service: None, // TODO
        })
    }
}

#[cfg(test)]
mod test {
    use tetra_core::BitBuffer;

    #[test]
    fn test_parse_sn_activate_pdp_context_demand() {

        let test_vec = "000000010100000000010001001000100000000101000100111100100000011000010001000110001110100000001000111000000000000011101000011110010111101111000011110101001000101010000010001000111101110000110101111001100110110011001000000100001101001111100110011100100010001001001010011010100010101010100010100100100000101011111010100001100001";
        let mut buf_in = BitBuffer::from_bitstr(test_vec);

            let pdu = super::SNActivatePdpContextDemand::from_bitbuf(&mut buf_in, tetra_core::Direction::Ul).unwrap();

            println!("{:#?}", pdu);
    }

}