/// Reject values that cannot safely be used as a DNS name.
pub fn validate_domain(domain: &str) -> Result<(), &'static str> {
    if domain.is_empty()
        || domain.len() > 253
        || domain.split('.').any(|label| {
            label.is_empty()
                || label.len() > 63
                || label.starts_with('-')
                || label.ends_with('-')
                || !label
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-')
        })
    {
        return Err("Invalid remote domain");
    }
    Ok(())
}

pub fn build_local_project_url(host: &str, port: u16, ingress: u16, token: Option<&str>) -> String {
    let mut url = format!("http://{port}.{host}.devvm.localhost:{ingress}");
    if let Some(token) = token {
        url.push_str(&format!("?token={token}"));
    }
    url
}

pub fn build_remote_project_url(
    host: &str,
    port: u16,
    domain: &str,
    token: Option<&str>,
) -> Result<String, &'static str> {
    if port == 0 {
        return Err("Invalid port");
    }
    let mut url = build_remote_port_template(host, domain)?.replace("{port}", &port.to_string());
    if let Some(token) = token {
        url.push_str(&format!("?token={token}"));
    }
    Ok(url)
}

pub fn build_local_port_template(host: &str, ingress: u16) -> String {
    format!("http://{{port}}.{host}.devvm.localhost:{ingress}")
}

pub fn build_remote_port_template(host: &str, domain: &str) -> Result<String, &'static str> {
    validate_domain(domain)?;
    // Project identity is assigned once by compute_project_host and devvm, never changed here.
    if host.is_empty()
        || host.len() > 57
        || host.starts_with('-')
        || host.ends_with('-')
        || !host.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
        || host.len() + 7 + domain.len() > 253
    {
        return Err("Invalid project hostname");
    }
    Ok(format!("https://{host}-{{port}}.{domain}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::compute_project_host;
    use std::path::Path;

    #[test]
    fn urls_preserve_routing_identity_and_validate_inputs() {
        for name in ["my-app".to_string(), "x".repeat(100)] {
            let path = format!("/projects/{name}");
            let host = compute_project_host(Path::new(&path));
            let template = build_remote_port_template(&host, "risak.dev").unwrap();
            for port in [1, 3080, 65535] {
                let url = build_remote_project_url(&host, port, "risak.dev", None).unwrap();
                assert_eq!(url, template.replace("{port}", &port.to_string()));
                assert_eq!(url, format!("https://{host}-{port}.risak.dev"));
            }
        }
        assert_eq!(
            build_remote_project_url("app-12345678", 3080, "risak.dev", Some("token")).unwrap(),
            "https://app-12345678-3080.risak.dev?token=token"
        );
        assert_eq!(
            build_local_project_url("app-12345678", 3080, 8102, None),
            "http://3080.app-12345678.devvm.localhost:8102"
        );
        assert!(build_remote_project_url("app", 0, "risak.dev", None).is_err());
        for domain in ["", "bad domain", "-bad.dev", "bad..dev", "bad.dev\n}"] {
            assert!(validate_domain(domain).is_err());
        }
        assert!(validate_domain(&format!("{}.dev", "x".repeat(64))).is_err());
        assert!(build_remote_port_template(&"x".repeat(58), "risak.dev").is_err());
    }
}
