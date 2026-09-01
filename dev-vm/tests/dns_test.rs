mod common;

use common::{build_dns_query, parse_dns_response};
use devvm_daemon::{DnsConfig, DnsServer};
use std::net::{Ipv4Addr, Ipv6Addr};
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::sync::watch;

#[tokio::test]
async fn test_dns_server_resolution_and_wildcards() {
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let server_addr = socket.local_addr().unwrap();

    let target_ip = Ipv4Addr::new(100, 64, 10, 20);
    let target_ipv6 = "fd7a:115c:a1e0::2".parse::<Ipv6Addr>().unwrap();

    let config = DnsConfig {
        bind_addr: server_addr.to_string(),
        target_ip,
        domain: "devvm.internal".to_string(),
        target_ipv6: Some(target_ipv6),
        ttl: 120,
    };

    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    tokio::spawn(async move {
        DnsServer::run_with_socket(socket, config, Some(shutdown_rx))
            .await
            .unwrap();
    });

    let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let mut recv_buf = vec![0u8; 1024];

    // Case 1: Query for subdomain "foo.devvm.internal" (Type A = 1)
    let query = build_dns_query(0x1001, "foo.devvm.internal", 1);
    client_socket.send_to(&query, server_addr).await.unwrap();

    let (len, _) = tokio::time::timeout(
        Duration::from_secs(1),
        client_socket.recv_from(&mut recv_buf),
    )
    .await
    .expect("DNS response timeout")
    .unwrap();

    let resp = parse_dns_response(&recv_buf[..len]);
    assert_eq!(resp.tx_id, 0x1001);
    assert_eq!(resp.rcode, 0); // NoError
    assert!(resp.is_authoritative);
    assert_eq!(resp.ancount, 1);
    assert_eq!(resp.a_records, vec![target_ip]);

    // Case 2: Query for deep subdomain "3000.my-proj-12345678.devvm.internal" (Type A = 1)
    let query = build_dns_query(0x1002, "3000.my-proj-12345678.devvm.internal", 1);
    client_socket.send_to(&query, server_addr).await.unwrap();

    let (len, _) = tokio::time::timeout(
        Duration::from_secs(1),
        client_socket.recv_from(&mut recv_buf),
    )
    .await
    .expect("DNS response timeout")
    .unwrap();

    let resp = parse_dns_response(&recv_buf[..len]);
    assert_eq!(resp.tx_id, 0x1002);
    assert_eq!(resp.rcode, 0);
    assert_eq!(resp.ancount, 1);
    assert_eq!(resp.a_records, vec![target_ip]);

    // Case 3: Query for base domain "devvm.internal" (Type A = 1)
    let query = build_dns_query(0x1003, "devvm.internal", 1);
    client_socket.send_to(&query, server_addr).await.unwrap();

    let (len, _) = tokio::time::timeout(
        Duration::from_secs(1),
        client_socket.recv_from(&mut recv_buf),
    )
    .await
    .expect("DNS response timeout")
    .unwrap();

    let resp = parse_dns_response(&recv_buf[..len]);
    assert_eq!(resp.tx_id, 0x1003);
    assert_eq!(resp.rcode, 0);
    assert_eq!(resp.ancount, 1);
    assert_eq!(resp.a_records, vec![target_ip]);

    // Case 4: Query for AAAA on matched domain (Type AAAA = 28)
    let query = build_dns_query(0x1004, "3000.my-proj-12345678.devvm.internal", 28);
    client_socket.send_to(&query, server_addr).await.unwrap();

    let (len, _) = tokio::time::timeout(
        Duration::from_secs(1),
        client_socket.recv_from(&mut recv_buf),
    )
    .await
    .expect("DNS response timeout")
    .unwrap();

    let resp = parse_dns_response(&recv_buf[..len]);
    assert_eq!(resp.tx_id, 0x1004);
    assert_eq!(resp.rcode, 0);
    assert_eq!(resp.ancount, 1);
    assert_eq!(resp.aaaa_records, vec![target_ipv6]);

    // Case 5: Non-matching domain "example.com" -> NXDomain (rcode 3)
    let query = build_dns_query(0x1005, "example.com", 1);
    client_socket.send_to(&query, server_addr).await.unwrap();

    let (len, _) = tokio::time::timeout(
        Duration::from_secs(1),
        client_socket.recv_from(&mut recv_buf),
    )
    .await
    .expect("DNS response timeout")
    .unwrap();

    let resp = parse_dns_response(&recv_buf[..len]);
    assert_eq!(resp.tx_id, 0x1005);
    assert_eq!(resp.rcode, 3); // NXDomain
    assert_eq!(resp.ancount, 0);

    // Case 6: Non-matching lookalike domain "notdevvm.internal" -> NXDomain
    let query = build_dns_query(0x1006, "notdevvm.internal", 1);
    client_socket.send_to(&query, server_addr).await.unwrap();

    let (len, _) = tokio::time::timeout(
        Duration::from_secs(1),
        client_socket.recv_from(&mut recv_buf),
    )
    .await
    .expect("DNS response timeout")
    .unwrap();

    let resp = parse_dns_response(&recv_buf[..len]);
    assert_eq!(resp.tx_id, 0x1006);
    assert_eq!(resp.rcode, 3); // NXDomain
    assert_eq!(resp.ancount, 0);

    let _ = shutdown_tx.send(true);

    // Case 7: Matched AAAA query without configured IPv6 target returns NODATA.
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let server_addr = socket.local_addr().unwrap();
    let config = DnsConfig {
        bind_addr: server_addr.to_string(),
        target_ip,
        domain: "devvm.internal".to_string(),
        target_ipv6: None,
        ttl: 120,
    };
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    tokio::spawn(async move {
        DnsServer::run_with_socket(socket, config, Some(shutdown_rx))
            .await
            .unwrap();
    });

    let query = build_dns_query(0x1007, "devvm.internal", 28);
    client_socket.send_to(&query, server_addr).await.unwrap();
    let (len, _) = tokio::time::timeout(
        Duration::from_secs(1),
        client_socket.recv_from(&mut recv_buf),
    )
    .await
    .expect("DNS response timeout")
    .unwrap();
    let resp = parse_dns_response(&recv_buf[..len]);
    assert_eq!(resp.tx_id, 0x1007);
    assert_eq!(resp.rcode, 0);
    assert_eq!(resp.ancount, 0);

    let _ = shutdown_tx.send(true);
}
