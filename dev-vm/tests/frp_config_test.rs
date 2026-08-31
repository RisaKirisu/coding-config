use std::fs;
use std::process::Command;
use toml::Value;

#[test]
fn test_frpc_config_structure_and_domains() {
    let content = fs::read_to_string("scripts/frpc.toml").expect("scripts/frpc.toml must exist");

    let parsed: Value = toml::from_str(&content).expect("scripts/frpc.toml must be valid TOML");

    // 1. Verify server connection settings
    assert!(parsed.get("serverAddr").is_some());
    assert!(parsed.get("serverPort").is_some());

    // 2. Verify proxies table array
    let proxies = parsed
        .get("proxies")
        .and_then(|p| p.as_array())
        .expect("proxies must be an array of tables");
    assert!(!proxies.is_empty(), "proxies list must not be empty");

    let proxy = &proxies[0];

    // Proxy name template
    let name = proxy
        .get("name")
        .and_then(|n| n.as_str())
        .expect("proxy name must be a string");
    assert!(name.contains("DEVVM_PROJECT_HOST"));

    // Proxy type
    let ptype = proxy
        .get("type")
        .and_then(|t| t.as_str())
        .expect("proxy type must be a string");
    assert_eq!(ptype, "http");

    // Local target
    let local_ip = proxy
        .get("localIP")
        .and_then(|ip| ip.as_str())
        .expect("proxy localIP must be a string");
    assert_eq!(local_ip, "127.0.0.1");

    let local_port = proxy
        .get("localPort")
        .and_then(|p| p.as_integer())
        .expect("proxy localPort must be an integer");
    assert_eq!(local_port, 10080);

    // Custom domains: Must include both .devvm.localhost and .devvm.internal
    let custom_domains = proxy
        .get("customDomains")
        .and_then(|cd| cd.as_array())
        .expect("customDomains must be an array");

    let domain_strings: Vec<&str> = custom_domains.iter().filter_map(|d| d.as_str()).collect();

    assert!(
        domain_strings
            .iter()
            .any(|d| d.contains(".devvm.localhost")),
        "customDomains must contain .devvm.localhost wildcard"
    );
    assert!(
        domain_strings.iter().any(|d| d.contains(".devvm.internal")),
        "customDomains must contain .devvm.internal wildcard"
    );
}

#[test]
fn test_frpc_verify_execution() {
    // If frpc binary exists on system, test frpc verify -c scripts/frpc.toml
    let has_frpc = Command::new("which")
        .arg("frpc")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !has_frpc {
        println!("frpc binary not found, skipping cli verification");
        return;
    }

    let output = Command::new("frpc")
        .arg("verify")
        .arg("-c")
        .arg("scripts/frpc.toml")
        .env("DEVVM_PROJECT_HOST", "smoke-test-project")
        .output()
        .expect("Failed to execute frpc verify");

    assert!(
        output.status.success(),
        "frpc verify failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
