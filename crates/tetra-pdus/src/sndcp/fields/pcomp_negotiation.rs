use core::fmt;
use tetra_core::{BitBuffer, PduParseErr};

/// 28.4.5.28 PCOMP negotiation
#[derive(Debug, Clone)]
pub struct PCOMPNegotiation {
    /// Van Jacobson TCP/IP header compression
    pub vj_tcp_ip_header_compression: bool,

    /// IP header compression
    pub ip_header_compression: bool,

    /// Other header compression (RTP & UDP)
    pub other_header_compression: bool,

    // Other 5 bits are reserved
}

impl PCOMPNegotiation {

    pub fn from_bitbuf(buf: &mut BitBuffer) -> Result<Self, PduParseErr> {
        let s = PCOMPNegotiation {
            vj_tcp_ip_header_compression: buf.read_field(1, "vj_tcp_ip_header_compression")? != 0,
            ip_header_compression: buf.read_field(1, "ip_header_compression")? != 0,
            other_header_compression: buf.read_field(1, "other_header_compression")? != 0,
        };

        // Skip reserved bits
        buf.read_bits(5);

        Ok(s)
    }

    pub fn to_bitbuf(&self, buf: &mut BitBuffer) {
        buf.write_bits(self.vj_tcp_ip_header_compression as u64, 1);
        buf.write_bits(self.ip_header_compression as u64, 1);
        buf.write_bits(self.other_header_compression as u64, 1);

        // Write reserved bits
        buf.write_bits(0, 5);
    }
}

impl fmt::Display for PCOMPNegotiation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PCOMP Negotiation: vj_tcp_ip_header_compression={}, ip_header_compression={}, other_header_compression={}",
            self.vj_tcp_ip_header_compression,
            self.ip_header_compression,
            self.other_header_compression,
        )
    }
}