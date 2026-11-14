//! Fixed read-only host inventory; never run shell fragments from configuration.
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::{
    io::Read,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
fn bounded(program: &str, args: &[&str]) -> Result<Vec<u8>> {
    let mut child = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut pipe = child.stdout.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = pipe.by_ref().take(262145).read_to_end(&mut bytes);
        (result, bytes)
    });
    let end = Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(s) = child.try_wait()? {
            break s;
        }
        if Instant::now() >= end {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!("Inventory tool timed out");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let (result, bytes) = reader
        .join()
        .map_err(|_| anyhow::anyhow!("Inventory reader failed"))?;
    result?;
    ensure!(
        status.success() && bytes.len() <= 262144,
        "Inventory failed or exceeded its output bound"
    );
    Ok(bytes)
}
pub fn host() -> Result<Value> {
    let links: Value =
        serde_json::from_slice(&bounded("ip", &["-json", "-details", "link", "show"])?)?;
    let addresses: Value = serde_json::from_slice(&bounded("ip", &["-json", "address", "show"])?)?;
    let routes: Value =
        serde_json::from_slice(&bounded("ip", &["-json", "route", "show", "table", "all"])?)?;
    let mut managers = serde_json::Map::new();
    for name in [
        "systemd-networkd",
        "NetworkManager",
        "networking",
        "kea-dhcp4-server",
        "unbound",
        "knot",
        "frr",
    ] {
        let state = bounded(
            "systemctl",
            &["show", name, "--property=ActiveState", "--value"],
        )
        .ok()
        .and_then(|b| String::from_utf8(b).ok())
        .map(|s| s.trim().to_owned())
        .unwrap_or_else(|| "unavailable".into());
        managers.insert(name.into(), json!(state));
    }
    Ok(
        json!({"schema":1,"read_only":true,"links":links,"addresses":addresses,"routes":routes,"service_states":managers,"physical_device_writes":false}),
    )
}
