use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::watch;
use tracing::{info, warn};

pub fn detect_tailscale_ipv4() -> Option<Ipv4Addr> {
    let output = std::process::Command::new("tailscale")
        .args(["ip", "-4"])
        .output()
        .ok()?;
    if output.status.success() {
        let ip_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
        ip_str.parse::<Ipv4Addr>().ok()
    } else {
        None
    }
}

#[derive(Clone, Debug)]
pub struct DnsConfig {
    pub bind_addr: String,
    pub target_ip: Ipv4Addr,
    pub domain: String,
    pub target_ipv6: Option<Ipv6Addr>,
    pub ttl: u32,
}

impl Default for DnsConfig {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0:53".to_string(),
            target_ip: Ipv4Addr::new(127, 0, 0, 1),
            domain: "devvm.internal".to_string(),
            target_ipv6: None,
            ttl: 60,
        }
    }
}

pub struct DnsServer {
    config: DnsConfig,
}

impl DnsServer {
    pub fn new(config: DnsConfig) -> Self {
        Self { config }
    }

    pub async fn run(&self) -> Result<(), std::io::Error> {
        let socket = UdpSocket::bind(&self.config.bind_addr).await?;
        info!("DNS server listening on {}", self.config.bind_addr);
        Self::serve_loop(Arc::new(socket), self.config.clone(), None).await
    }

    pub async fn run_with_socket(
        socket: UdpSocket,
        config: DnsConfig,
        mut shutdown_rx: Option<watch::Receiver<bool>>,
    ) -> Result<(), std::io::Error> {
        Self::serve_loop(Arc::new(socket), config, shutdown_rx.as_mut()).await
    }

    async fn serve_loop(
        socket: Arc<UdpSocket>,
        config: DnsConfig,
        mut shutdown: Option<&mut watch::Receiver<bool>>,
    ) -> Result<(), std::io::Error> {
        let mut buf = vec![0u8; 4096];

        loop {
            tokio::select! {
                res = socket.recv_from(&mut buf) => {
                    match res {
                        Ok((len, src)) => {
                            let query_data = &buf[..len];
                            if let Some(resp) = handle_dns_query(query_data, &config) {
                                let _ = socket.send_to(&resp, src).await;
                            }
                        }
                        Err(e) => {
                            warn!("DNS recv error: {}", e);
                        }
                    }
                }
                _ = async {
                    if let Some(rx) = shutdown.as_deref_mut() {
                        let _ = rx.changed().await;
                    } else {
                        std::future::pending::<()>().await;
                    }
                } => {
                    info!("DNS server shutting down");
                    break;
                }
            }
        }
        Ok(())
    }
}

pub fn handle_dns_query(data: &[u8], config: &DnsConfig) -> Option<Vec<u8>> {
    if data.len() < 12 {
        return None;
    }

    let tx_id = &data[0..2];
    let flags = u16::from_be_bytes([data[2], data[3]]);
    let qr = (flags >> 15) & 1;
    let opcode = (flags >> 11) & 0xF;
    let qdcount = u16::from_be_bytes([data[4], data[5]]);

    // Only process standard queries (QR=0, Opcode=0) with at least 1 question
    if qr != 0 || opcode != 0 || qdcount == 0 {
        return None;
    }

    let mut pos = 12;
    let mut labels = Vec::new();

    while pos < data.len() {
        let len = data[pos] as usize;
        if len == 0 {
            pos += 1;
            break;
        }
        if (len & 0xC0) == 0xC0 {
            // Pointer in query question
            pos += 2;
            break;
        } else if len <= 63 {
            pos += 1;
            if pos + len > data.len() {
                return None;
            }
            if let Ok(label_str) = std::str::from_utf8(&data[pos..pos + len]) {
                labels.push(label_str.to_string());
            } else {
                return None;
            }
            pos += len;
        } else {
            return None;
        }
    }

    if pos + 4 > data.len() {
        return None;
    }

    let qtype = u16::from_be_bytes([data[pos], data[pos + 1]]);
    let _qclass = u16::from_be_bytes([data[pos + 2], data[pos + 3]]);
    let question_bytes = &data[12..pos + 4];
    let qname = labels.join(".");

    let q_clean = qname.trim_end_matches('.').to_ascii_lowercase();
    let d_clean = config.domain.trim_end_matches('.').to_ascii_lowercase();
    let matches = q_clean == d_clean || q_clean.ends_with(&format!(".{}", d_clean));

    let mut resp = Vec::with_capacity(512);
    resp.extend_from_slice(tx_id);

    // Flags: QR=1 (response), AA=1 (authoritative), RD preserved from query
    let flag_high = 0x84 | (data[2] & 0x01);
    let flag_low = if matches { 0x00 } else { 0x03 }; // 0 = NoError, 3 = NXDomain
    resp.push(flag_high);
    resp.push(flag_low);

    // QDCOUNT = 1
    resp.extend_from_slice(&1u16.to_be_bytes());

    // ANCOUNT
    let mut is_a_answer = false;
    let mut is_aaaa_answer = false;

    if matches {
        if qtype == 1 || qtype == 255 {
            // A record or ANY query
            is_a_answer = true;
        } else if qtype == 28 {
            // AAAA record query
            if config.target_ipv6.is_some() {
                is_aaaa_answer = true;
            }
        }
    }

    let ancount: u16 = if is_a_answer || is_aaaa_answer { 1 } else { 0 };
    resp.extend_from_slice(&ancount.to_be_bytes());
    // NSCOUNT = 0
    resp.extend_from_slice(&0u16.to_be_bytes());
    // ARCOUNT = 0
    resp.extend_from_slice(&0u16.to_be_bytes());

    // Question section
    resp.extend_from_slice(question_bytes);

    // Answer section
    if is_a_answer {
        // Name pointer to question at offset 12: 0xC0, 0x0C
        resp.push(0xC0);
        resp.push(0x0C);
        // TYPE A (1)
        resp.extend_from_slice(&1u16.to_be_bytes());
        // CLASS IN (1)
        resp.extend_from_slice(&1u16.to_be_bytes());
        // TTL
        resp.extend_from_slice(&config.ttl.to_be_bytes());
        // RDLENGTH 4
        resp.extend_from_slice(&4u16.to_be_bytes());
        // RDATA
        resp.extend_from_slice(&config.target_ip.octets());
    } else if is_aaaa_answer {
        if let Some(ipv6) = config.target_ipv6 {
            // Name pointer to question at offset 12: 0xC0, 0x0C
            resp.push(0xC0);
            resp.push(0x0C);
            // TYPE AAAA (28)
            resp.extend_from_slice(&28u16.to_be_bytes());
            // CLASS IN (1)
            resp.extend_from_slice(&1u16.to_be_bytes());
            // TTL
            resp.extend_from_slice(&config.ttl.to_be_bytes());
            // RDLENGTH 16
            resp.extend_from_slice(&16u16.to_be_bytes());
            // RDATA
            resp.extend_from_slice(&ipv6.octets());
        }
    }

    Some(resp)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_raw_query(qname: &str, qtype: u16) -> Vec<u8> {
        let mut packet = Vec::new();
        // ID: 0x1234
        packet.extend_from_slice(&0x1234u16.to_be_bytes());
        // Flags: Standard query, RD=1
        packet.extend_from_slice(&0x0100u16.to_be_bytes());
        // QDCOUNT: 1
        packet.extend_from_slice(&1u16.to_be_bytes());
        // ANCOUNT, NSCOUNT, ARCOUNT = 0
        packet.extend_from_slice(&[0, 0, 0, 0, 0, 0]);

        for part in qname.split('.') {
            packet.push(part.len() as u8);
            packet.extend_from_slice(part.as_bytes());
        }
        packet.push(0); // Root label

        // QTYPE
        packet.extend_from_slice(&qtype.to_be_bytes());
        // QCLASS IN (1)
        packet.extend_from_slice(&1u16.to_be_bytes());

        packet
    }

    #[test]
    fn test_dns_query_a_exact_and_subdomain() {
        let config = DnsConfig {
            bind_addr: "127.0.0.1:0".to_string(),
            target_ip: Ipv4Addr::new(100, 64, 1, 2),
            domain: "devvm.internal".to_string(),
            target_ipv6: None,
            ttl: 60,
        };

        // 1. Exact domain
        let query = build_raw_query("devvm.internal", 1);
        let resp = handle_dns_query(&query, &config).expect("Response generated");
        assert_eq!(&resp[0..2], &0x1234u16.to_be_bytes());
        assert_eq!(resp[3] & 0x0F, 0); // NoError
        assert_eq!(u16::from_be_bytes([resp[6], resp[7]]), 1); // ANCOUNT = 1
                                                               // Answer RDATA should end with target IP
        assert_eq!(&resp[resp.len() - 4..], &[100, 64, 1, 2]);

        // 2. Subdomain with port and project host
        let query2 = build_raw_query("3000.my-proj-12345678.devvm.internal", 1);
        let resp2 = handle_dns_query(&query2, &config).expect("Response generated");
        assert_eq!(resp2[3] & 0x0F, 0); // NoError
        assert_eq!(u16::from_be_bytes([resp2[6], resp2[7]]), 1);
        assert_eq!(&resp2[resp2.len() - 4..], &[100, 64, 1, 2]);
    }

    #[test]
    fn test_dns_query_non_matching_domain() {
        let config = DnsConfig {
            bind_addr: "127.0.0.1:0".to_string(),
            target_ip: Ipv4Addr::new(100, 64, 1, 2),
            domain: "devvm.internal".to_string(),
            target_ipv6: None,
            ttl: 60,
        };

        let query = build_raw_query("example.com", 1);
        let resp = handle_dns_query(&query, &config).expect("Response generated");
        assert_eq!(resp[3] & 0x0F, 3); // NXDomain
        assert_eq!(u16::from_be_bytes([resp[6], resp[7]]), 0); // ANCOUNT = 0
    }

    #[test]
    fn test_dns_query_aaaa_nodata() {
        let config = DnsConfig {
            bind_addr: "127.0.0.1:0".to_string(),
            target_ip: Ipv4Addr::new(100, 64, 1, 2),
            domain: "devvm.internal".to_string(),
            target_ipv6: None,
            ttl: 60,
        };

        let query = build_raw_query("devvm.internal", 28); // AAAA
        let resp = handle_dns_query(&query, &config).expect("Response generated");
        assert_eq!(resp[3] & 0x0F, 0); // NoError (NODATA)
        assert_eq!(u16::from_be_bytes([resp[6], resp[7]]), 0); // ANCOUNT = 0
    }

    #[test]
    fn test_dns_query_aaaa_with_target() {
        let ipv6 = "fd7a:115c:a1e0::1".parse::<Ipv6Addr>().unwrap();
        let config = DnsConfig {
            bind_addr: "127.0.0.1:0".to_string(),
            target_ip: Ipv4Addr::new(100, 64, 1, 2),
            domain: "devvm.internal".to_string(),
            target_ipv6: Some(ipv6),
            ttl: 60,
        };

        let query = build_raw_query("devvm.internal", 28); // AAAA
        let resp = handle_dns_query(&query, &config).expect("Response generated");
        assert_eq!(resp[3] & 0x0F, 0); // NoError
        assert_eq!(u16::from_be_bytes([resp[6], resp[7]]), 1); // ANCOUNT = 1
        assert_eq!(&resp[resp.len() - 16..], &ipv6.octets());
    }
}
