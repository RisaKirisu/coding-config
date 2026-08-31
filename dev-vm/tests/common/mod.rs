#![allow(dead_code)]

use async_trait::async_trait;
use axum::{
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json},
};
use devvm_daemon::{SyncConfig, SyncError, SyncRunner};
use serde_json::json;
use std::fs::{self, File};
use std::io::Write;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Child;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tokio::sync::Mutex;
use uuid::Uuid;

/// Mock `SyncRunner` implementation tracking all sync operations in atomic counters
/// and providing failure/reachability toggles.
pub struct MockSyncRunner {
    pub vps_reachable: AtomicBool,
    pub rsync_fails: AtomicBool,
    pub verify_count: AtomicUsize,
    pub push_count: AtomicUsize,
    pub pull_count: AtomicUsize,
    pub delete_count: AtomicUsize,
    pub deleted_projects: Mutex<Vec<Uuid>>,
}

impl MockSyncRunner {
    pub fn new() -> Self {
        Self {
            vps_reachable: AtomicBool::new(true),
            rsync_fails: AtomicBool::new(false),
            verify_count: AtomicUsize::new(0),
            push_count: AtomicUsize::new(0),
            pull_count: AtomicUsize::new(0),
            delete_count: AtomicUsize::new(0),
            deleted_projects: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl SyncRunner for MockSyncRunner {
    async fn verify_connection(&self, _config: &SyncConfig) -> Result<(), SyncError> {
        self.verify_count.fetch_add(1, Ordering::SeqCst);
        if self.vps_reachable.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(SyncError::ConnectionFailed(
                "Connection refused to VPS Sync Store".to_string(),
            ))
        }
    }

    async fn run_rsync_push(
        &self,
        _config: &SyncConfig,
        _project_id: Uuid,
        _project_path: &Path,
    ) -> Result<(), SyncError> {
        self.push_count.fetch_add(1, Ordering::SeqCst);
        if !self.vps_reachable.load(Ordering::SeqCst) || self.rsync_fails.load(Ordering::SeqCst) {
            Err(SyncError::PushFailed(
                "rsync push connection timed out".to_string(),
            ))
        } else {
            Ok(())
        }
    }

    async fn run_rsync_pull(
        &self,
        _config: &SyncConfig,
        _project_id: Uuid,
        project_path: &Path,
    ) -> Result<(), SyncError> {
        self.pull_count.fetch_add(1, Ordering::SeqCst);
        if !self.vps_reachable.load(Ordering::SeqCst) || self.rsync_fails.load(Ordering::SeqCst) {
            Err(SyncError::PullFailed(
                "rsync pull connection timed out".to_string(),
            ))
        } else {
            // Simulate pulling some sessions
            let sessions_dir = project_path.join(".dsh/sessions");
            let _ = fs::create_dir_all(&sessions_dir);
            let _ = fs::write(sessions_dir.join("synced-session.jsonl"), "{}\n");
            Ok(())
        }
    }

    async fn delete_remote_store(
        &self,
        _config: &SyncConfig,
        project_id: Uuid,
    ) -> Result<(), SyncError> {
        self.delete_count.fetch_add(1, Ordering::SeqCst);
        if !self.vps_reachable.load(Ordering::SeqCst) {
            Err(SyncError::DeletionFailed(
                "Failed to connect to VPS for deletion".to_string(),
            ))
        } else {
            let mut list = self.deleted_projects.lock().await;
            list.push(project_id);
            Ok(())
        }
    }

    async fn is_local_state_dirty(&self, project_path: &Path) -> Result<bool, SyncError> {
        Ok(devvm_daemon::is_local_state_dirty(
            &project_path.join(".dsh"),
        ))
    }

    async fn mark_local_state_dirty(&self, project_path: &Path) -> Result<(), SyncError> {
        devvm_daemon::mark_local_state_dirty(&project_path.join(".dsh")).map_err(SyncError::IoError)
    }

    async fn mark_local_state_clean(&self, project_path: &Path) -> Result<(), SyncError> {
        devvm_daemon::mark_local_state_clean(&project_path.join(".dsh")).map_err(SyncError::IoError)
    }

    async fn check_local_portable_state_exists(
        &self,
        project_path: &Path,
    ) -> Result<bool, SyncError> {
        Ok(devvm_daemon::check_local_portable_state_exists(
            &project_path.join(".dsh"),
        ))
    }

    async fn get_in_vm_sync_status(
        &self,
        _project_path: &Path,
    ) -> Result<Option<devvm_daemon::models::SyncStatus>, SyncError> {
        Ok(None)
    }

    async fn set_in_vm_sync_status(
        &self,
        _project_path: &Path,
        _status: devvm_daemon::models::SyncStatus,
        _is_dirty: bool,
    ) -> Result<(), SyncError> {
        Ok(())
    }
}

/// Creates a mock executable bash script for `devvm` CLI that simulates VM lifecycle
/// and DSH execution with optional failure injection hooks.
pub fn create_mock_devvm(bin_path: &Path) {
    let script = r#"#!/usr/bin/env bash
cmd="${1:-status}"
shift || true

case "$cmd" in
    status)
        if [[ -f ".vm_running" ]]; then
            echo "running"
            exit 0
        else
            echo "stopped"
            exit 1
        fi
        ;;
    start)
        touch ".vm_running"
        echo "Mock DevVM: started"
        exit 0
        ;;
    stop)
        rm -f ".vm_running"
        echo "Mock DevVM: stopped"
        exit 0
        ;;
    rm|delete)
        rm -f ".vm_running"
        echo "Mock DevVM: removed"
        exit 0
        ;;
    exec)
        if [[ "${1:-}" == "--" ]]; then
            shift
        fi
        if [[ "$1" == "dsh" && "$2" == "web" ]] || [[ "$*" == *"dsh web"* ]]; then
            if [[ -f ".dsh_start_slow" ]]; then
                sleep 0.4
            fi
            echo "dsh web: http://127.0.0.1:3080"
            if [[ -f ".dsh_fail_fast" ]]; then
                echo "Mock DSH: simulated crash" >&2
                exit 2
            fi
            if [[ -f ".dsh_fail_short" ]]; then
                sleep 0.3
                echo "Mock DSH: simulated runtime failure" >&2
                exit 3
            fi
            # Keep running until killed
            while true; do
                sleep 0.1
            done
        fi

        VM_DSH="$PWD/.mock_dsh"
        mkdir -p "$VM_DSH"

        echo "$@" >> "$PWD/.mock_exec_invocations" 2>/dev/null || true

        if [[ "$1" == "/bin/sh" || "$1" == "/bin/bash" ]] && [[ "$2" == "-c" ]]; then
            cmd_body="$3"
            mapped_cmd="${cmd_body//\/root\/.dsh/$VM_DSH}"
            eval "$mapped_cmd"
            exit $?
        fi

        if [[ "$1" == "rsync" ]]; then
            if [[ -f ".mock_sync_fail" ]]; then
                echo "Mock rsync: simulated failure" >&2
                exit 1
            fi
            # If pulling from remote into /root/.dsh/:
            if [[ "$*" == *"/root/.dsh/"* && "$*" != *"/root/.dsh/ "* ]]; then
                mkdir -p "$VM_DSH/sessions"
                echo "{}" > "$VM_DSH/sessions/synced-session.jsonl"
            fi
            exit 0
        fi

        echo "Mock DevVM exec: $@"
        exit 0
        ;;
    *)
        echo "Unknown command: $cmd" >&2
        exit 1
        ;;
esac
"#;
    let mut file = File::create(bin_path).unwrap();
    file.write_all(script.as_bytes()).unwrap();
    let mut perms = file.metadata().unwrap().permissions();
    perms.set_mode(0o755);
    file.set_permissions(perms).unwrap();
}

/// Builds a binary DNS query packet for standard UDP DNS servers.
pub fn build_dns_query(tx_id: u16, qname: &str, qtype: u16) -> Vec<u8> {
    let mut packet = Vec::new();
    // ID (2 bytes)
    packet.extend_from_slice(&tx_id.to_be_bytes());
    // Flags (2 bytes): Standard query, RD=1
    packet.extend_from_slice(&0x0100u16.to_be_bytes());
    // QDCOUNT: 1
    packet.extend_from_slice(&1u16.to_be_bytes());
    // ANCOUNT: 0, NSCOUNT: 0, ARCOUNT: 0
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

/// Parsed representation of a DNS response packet.
pub struct ParsedDnsResponse {
    pub tx_id: u16,
    pub rcode: u8,
    pub is_authoritative: bool,
    pub ancount: u16,
    pub a_records: Vec<Ipv4Addr>,
    pub aaaa_records: Vec<Ipv6Addr>,
}

/// Parses a binary DNS response packet into a `ParsedDnsResponse` struct.
pub fn parse_dns_response(data: &[u8]) -> ParsedDnsResponse {
    assert!(data.len() >= 12, "DNS response too short");
    let tx_id = u16::from_be_bytes([data[0], data[1]]);
    let flags = u16::from_be_bytes([data[2], data[3]]);
    let is_authoritative = (flags & 0x0400) != 0;
    let rcode = (flags & 0x000F) as u8;
    let qdcount = u16::from_be_bytes([data[4], data[5]]);
    let ancount = u16::from_be_bytes([data[6], data[7]]);

    // Skip question section
    let mut pos = 12;
    for _ in 0..qdcount {
        while pos < data.len() {
            let len = data[pos] as usize;
            if len == 0 {
                pos += 1;
                break;
            }
            if (len & 0xC0) == 0xC0 {
                pos += 2;
                break;
            }
            pos += 1 + len;
        }
        pos += 4; // qtype + qclass
    }

    let mut a_records = Vec::new();
    let mut aaaa_records = Vec::new();

    for _ in 0..ancount {
        if pos >= data.len() {
            break;
        }
        // Name (either pointer or label)
        if (data[pos] & 0xC0) == 0xC0 {
            pos += 2;
        } else {
            while pos < data.len() && data[pos] != 0 {
                pos += 1 + (data[pos] as usize);
            }
            pos += 1;
        }

        if pos + 10 > data.len() {
            break;
        }
        let atype = u16::from_be_bytes([data[pos], data[pos + 1]]);
        let _aclass = u16::from_be_bytes([data[pos + 2], data[pos + 3]]);
        let _ttl = u32::from_be_bytes([data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]]);
        let rdlen = u16::from_be_bytes([data[pos + 8], data[pos + 9]]) as usize;
        pos += 10;

        if pos + rdlen > data.len() {
            break;
        }

        if atype == 1 && rdlen == 4 {
            let ip = Ipv4Addr::new(data[pos], data[pos + 1], data[pos + 2], data[pos + 3]);
            a_records.push(ip);
        } else if atype == 28 && rdlen == 16 {
            let mut octets = [0u8; 16];
            octets.copy_from_slice(&data[pos..pos + 16]);
            aaaa_records.push(Ipv6Addr::from(octets));
        }

        pos += rdlen;
    }

    ParsedDnsResponse {
        tx_id,
        rcode,
        is_authoritative,
        ancount,
        a_records,
        aaaa_records,
    }
}

/// RAII Guard ensuring spawned child processes (like Caddy) are killed and reaped upon drop.
pub struct CaddyGuard(pub Option<Child>);

impl Drop for CaddyGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Axum handler that returns received HTTP headers in JSON response body.
pub async fn echo_headers_handler(headers: HeaderMap) -> impl IntoResponse {
    let mut map = serde_json::Map::new();
    for (k, v) in headers.iter() {
        if let Ok(v_str) = v.to_str() {
            map.insert(k.as_str().to_string(), json!(v_str));
        }
    }
    (StatusCode::OK, Json(map))
}
