use kagnode_netctl::{Network, ipam::Ipam};
use serde_json::json;
fn network() -> Network {
    serde_json::from_value(json!({"interfaces":[{"name":"eth0","link":{"kind":"physical"},"addresses":["192.0.2.10/24"]},{"name":"knha42","link":{"kind":"vlan","parent":"eth0","tag":42},"addresses":["10.42.0.1/24"]}],"routes":[{"destination":"0.0.0.0/0","gateway":"192.0.2.1","interface":"eth0"}],"dns_servers":["192.0.2.53"]})).unwrap()
}
#[test]
fn vlan_plan_is_deterministic_and_does_not_activate_networking() {
    let a = network().plan().unwrap();
    let b = network().plan().unwrap();
    assert_eq!(a.sha256, b.sha256);
    assert!(!a.live_activation);
    assert!(a.files["etc/systemd/network/70-kagnode-knha42.netdev"].contains("Id=42"));
    assert!(a.files["etc/systemd/network/70-kagnode-eth0.network"].contains("VLAN=knha42"));
}
#[test]
fn rejects_unsafe_interface_and_declaration_fields() {
    let mut v = serde_json::to_value(network()).unwrap();
    v["interfaces"][0]["name"] = json!("eth0\nExecStart=bad");
    assert!(
        serde_json::from_value::<Network>(v)
            .unwrap()
            .validate()
            .is_err()
    );
    let v = json!({"interfaces":[],"shell_hook":"anything"});
    assert!(serde_json::from_value::<Network>(v).is_err());
}
#[test]
fn rejects_duplicate_addresses_and_invalid_vlan_tags() {
    let mut n = network();
    n.interfaces[1].addresses = n.interfaces[0].addresses.clone();
    assert!(n.validate().is_err());
    let mut v = serde_json::to_value(network()).unwrap();
    v["interfaces"][1]["link"]["tag"] = json!(4095);
    assert!(
        serde_json::from_value::<Network>(v)
            .unwrap()
            .validate()
            .is_err()
    );
}
#[test]
fn render_does_not_overwrite_existing_files_or_follow_directory_links() {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("rendered");
    let p = network().plan().unwrap();
    p.write_new(&target).unwrap();
    let original = std::fs::read(target.join("plan.json")).unwrap();
    assert!(p.write_new(&target).is_err());
    assert_eq!(std::fs::read(target.join("plan.json")).unwrap(), original);
    std::os::unix::fs::symlink(&target, root.path().join("alias")).unwrap();
    assert!(p.write_new(&root.path().join("alias/next")).is_err());
}
#[test]
fn dns_txt_quotes_are_escaped_and_records_cannot_inject_lines() {
    let mut v = serde_json::to_value(network()).unwrap();
    v["zones"] = json!([{"name":"example.test","primary_ns":"ns.example.test","admin_mailbox":"hostmaster.example.test","records":[{"name":"@","kind":"TXT","value":"quoted \" text"}]}]);
    let n: Network = serde_json::from_value(v.clone()).unwrap();
    let p = n.plan().unwrap();
    assert!(p.files["etc/kagnode/netctl/zones/example.test.zone"].contains("\\\""));
    v["zones"][0]["records"][0]["value"] = json!("text\n@ IN A 1.2.3.4");
    assert!(
        serde_json::from_value::<Network>(v)
            .unwrap()
            .validate()
            .is_err()
    );
}
#[test]
fn dhcp_pool_cannot_allocate_static_or_router_addresses() {
    let mut v = serde_json::to_value(network()).unwrap();
    v["dhcp"] = json!([{"interface":"knha42","subnet":"10.42.0.0/24","start":"10.42.0.1","end":"10.42.0.50","router":"10.42.0.1","dns":["192.0.2.53"]}]);
    assert!(
        serde_json::from_value::<Network>(v.clone())
            .unwrap()
            .validate()
            .is_err()
    );
    v["dhcp"][0]["start"] = json!("10.42.0.20");
    assert!(
        serde_json::from_value::<Network>(v)
            .unwrap()
            .plan()
            .unwrap()
            .files
            .contains_key("etc/kagnode/netctl/kea-dhcp4.json")
    );
}
#[test]
fn ipam_allocations_are_idempotent_exclusive_and_persisted() {
    let root = tempfile::tempdir().unwrap();
    let mut a = Ipam::open(root.path()).unwrap();
    a.add_pool(
        "ha",
        "10.42.0.0/24".parse().unwrap(),
        "10.42.0.10".parse().unwrap(),
        "10.42.0.11".parse().unwrap(),
    )
    .unwrap();
    let first = a.allocate("ha", "primary").unwrap();
    assert_eq!(a.allocate("ha", "primary").unwrap().address, first.address);
    let second = a.allocate("ha", "standby").unwrap();
    assert_ne!(first.address, second.address);
    assert!(a.allocate("ha", "overflow").is_err());
    drop(a);
    let mut b = Ipam::open(root.path()).unwrap();
    assert_eq!(b.leases().unwrap().len(), 2);
    assert!(!b.release("ha", "another-owner", first.address).unwrap());
    assert!(b.release("ha", "primary", first.address).unwrap());
    assert_eq!(
        b.allocate("ha", "replacement").unwrap().address,
        first.address
    );
}
#[test]
fn ipam_rejects_overlapping_windows_and_supports_ipv6() {
    let root = tempfile::tempdir().unwrap();
    let mut a = Ipam::open(root.path()).unwrap();
    a.add_pool(
        "a",
        "fd42::/64".parse().unwrap(),
        "fd42::10".parse().unwrap(),
        "fd42::20".parse().unwrap(),
    )
    .unwrap();
    assert!(
        a.add_pool(
            "b",
            "fd42::/64".parse().unwrap(),
            "fd42::15".parse().unwrap(),
            "fd42::30".parse().unwrap()
        )
        .is_err()
    );
    assert!(a.allocate("a", "ipv6-node").unwrap().address.is_ipv6());
}

#[test]
fn vlan_cycles_and_undeclared_link_dependencies_are_rejected() {
    let mut v = serde_json::to_value(network()).unwrap();
    v["interfaces"][0]["link"] = json!({"kind":"bridge","ports":["knha42"]});
    let n: Network = serde_json::from_value(v).unwrap();
    assert!(n.validate().unwrap_err().to_string().contains("cycle"));
    let mut v = serde_json::to_value(network()).unwrap();
    v["interfaces"][1]["link"]["parent"] = json!("missing0");
    assert!(
        serde_json::from_value::<Network>(v)
            .unwrap()
            .plan()
            .is_err()
    );
}
#[test]
fn dhcp_excludes_network_broadcast_and_invalid_dns_options() {
    let mut v = serde_json::to_value(network()).unwrap();
    v["dhcp"] = json!([{"interface":"knha42","subnet":"10.42.0.0/24","start":"10.42.0.20","end":"10.42.0.255","router":"10.42.0.1"}]);
    assert!(
        serde_json::from_value::<Network>(v.clone())
            .unwrap()
            .validate()
            .is_err()
    );
    v["dhcp"][0]["end"] = json!("10.42.0.50");
    v["dhcp"][0]["dns"] = json!(["0.0.0.0"]);
    assert!(
        serde_json::from_value::<Network>(v)
            .unwrap()
            .validate()
            .is_err()
    );
}
#[test]
fn dns_zone_identity_ignores_case_and_root_dot() {
    let mut v = serde_json::to_value(network()).unwrap();
    let zone = json!({"name":"Example.test","primary_ns":"ns.example.test","admin_mailbox":"hostmaster.example.test","records":[]});
    let mut duplicate = zone.clone();
    duplicate["name"] = json!("example.test.");
    v["zones"] = json!([zone, duplicate]);
    assert!(
        serde_json::from_value::<Network>(v)
            .unwrap()
            .validate()
            .is_err()
    );
}
#[test]
fn ipam_does_not_repermission_existing_shared_directories() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("other-data"), "retained").unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(Ipam::open(root.path()).is_err());
    assert_eq!(
        root.path().metadata().unwrap().permissions().mode() & 0o777,
        0o755
    );
    assert_eq!(
        std::fs::read_to_string(root.path().join("other-data")).unwrap(),
        "retained"
    );
}
