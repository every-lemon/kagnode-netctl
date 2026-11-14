//! Typed network declarations. Rendering never changes the running network.
pub mod ipam;
pub mod observe;
use anyhow::{Result, ensure};
use ipnet::IpNet;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    net::{IpAddr, Ipv4Addr},
    path::Path,
};

pub fn name(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 15
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
}
fn dns_name(v: &str) -> bool {
    v.len() <= 253
        && !v.is_empty()
        && v.trim_end_matches('.').split('.').all(|l| {
            !l.is_empty()
                && l.len() <= 63
                && !l.starts_with('-')
                && !l.ends_with('-')
                && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Link {
    Physical,
    Vlan { parent: String, tag: u16 },
    Bridge { ports: Vec<String> },
    Bond { ports: Vec<String>, mode: BondMode },
}
#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BondMode {
    ActiveBackup,
    Ieee8023ad,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Interface {
    pub name: String,
    pub link: Link,
    #[serde(default)]
    pub addresses: Vec<IpNet>,
    #[serde(default)]
    pub dhcp: bool,
    #[serde(default = "default_mtu")]
    pub mtu: u32,
}
fn default_mtu() -> u32 {
    1500
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Route {
    pub destination: IpNet,
    pub gateway: IpAddr,
    pub interface: String,
    #[serde(default = "default_metric")]
    pub metric: u32,
}
fn default_metric() -> u32 {
    100
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dhcp {
    pub interface: String,
    pub subnet: IpNet,
    pub start: IpAddr,
    pub end: IpAddr,
    pub router: IpAddr,
    #[serde(default)]
    pub dns: Vec<IpAddr>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resolver {
    pub listen: Vec<IpAddr>,
    pub allow: Vec<IpNet>,
    #[serde(default)]
    pub forwarders: Vec<IpAddr>,
}
#[derive(Clone, Copy, Serialize, Deserialize)]
pub enum RecordType {
    A,
    AAAA,
    CNAME,
    MX,
    TXT,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub name: String,
    pub kind: RecordType,
    pub value: String,
    #[serde(default)]
    pub priority: u16,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Zone {
    pub name: String,
    pub primary_ns: String,
    pub admin_mailbox: String,
    pub records: Vec<Record>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bgp {
    pub asn: u32,
    pub router_id: Ipv4Addr,
    pub peers: Vec<BgpPeer>,
    pub announce: Vec<IpNet>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BgpPeer {
    pub address: IpAddr,
    pub asn: u32,
}
#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    Tcp,
    Udp,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortRule {
    pub interface: String,
    pub source: IpNet,
    pub protocol: Protocol,
    pub port: u16,
    pub accept: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Nat {
    pub source: IpNet,
    pub egress: String,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Network {
    pub interfaces: Vec<Interface>,
    #[serde(default)]
    pub routes: Vec<Route>,
    #[serde(default)]
    pub dns_servers: Vec<IpAddr>,
    #[serde(default)]
    pub dhcp: Vec<Dhcp>,
    #[serde(default)]
    pub resolver: Option<Resolver>,
    #[serde(default)]
    pub zones: Vec<Zone>,
    #[serde(default)]
    pub bgp: Option<Bgp>,
    #[serde(default)]
    pub firewall: Vec<PortRule>,
    #[serde(default)]
    pub nat: Vec<Nat>,
}
#[derive(Serialize)]
pub struct Plan {
    pub schema: u32,
    pub sha256: String,
    pub files: BTreeMap<String, String>,
    pub prerequisites: Vec<String>,
    pub review: Vec<String>,
    pub live_activation: bool,
}

impl Network {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.interfaces.len() <= 64
                && self.routes.len() <= 256
                && self.zones.len() <= 64
                && self.firewall.len() <= 512
                && self.dhcp.len() <= 64,
            "Network declaration exceeds supported limits"
        );
        let mut names = HashSet::new();
        let mut addresses = HashSet::new();
        for i in &self.interfaces {
            ensure!(
                name(&i.name) && names.insert(i.name.as_str()),
                "Duplicate or unsafe interface name"
            );
            ensure!(
                (576..=9216).contains(&i.mtu),
                "MTU must be between 576 and 9216"
            );
            ensure!(i.addresses.len() <= 32, "Too many interface addresses");
            for a in &i.addresses {
                ensure!(
                    !a.addr().is_unspecified()
                        && !a.addr().is_multicast()
                        && addresses.insert(a.addr()),
                    "Duplicate or non-unicast interface address"
                );
                ensure!(
                    a.addr().is_ipv4() || i.mtu >= 1280,
                    "IPv6 requires an MTU of at least 1280"
                );
            }
            match &i.link {
                Link::Vlan { parent, tag } => ensure!(
                    name(parent) && parent != &i.name && (1..=4094).contains(tag),
                    "Invalid VLAN parent or tag"
                ),
                Link::Bridge { ports } | Link::Bond { ports, .. } => {
                    let mut set = HashSet::new();
                    ensure!(
                        !ports.is_empty() && ports.len() <= 32,
                        "Choose bridge/bond member ports"
                    );
                    for p in ports {
                        ensure!(
                            name(p) && p != &i.name && set.insert(p),
                            "Invalid or duplicate member port"
                        );
                    }
                }
                _ => {}
            }
        }
        // Prevent bridge/bond double ownership and dependency cycles.
        let mut parents = BTreeMap::new();
        for i in &self.interfaces {
            if let Link::Bridge { ports } | Link::Bond { ports, .. } = &i.link {
                for p in ports {
                    ensure!(
                        parents.insert(p.as_str(), i.name.as_str()).is_none(),
                        "A port cannot belong to two bridges or bonds"
                    );
                }
            }
        }
        for i in &self.interfaces {
            let mut visited = HashSet::new();
            let mut current = i.name.as_str();
            while let Some(parent) = parents.get(current) {
                ensure!(visited.insert(current), "Network member cycle detected");
                current = parent;
            }
        }
        for r in &self.routes {
            ensure!(
                name(&r.interface)
                    && names.contains(r.interface.as_str())
                    && r.destination.addr().is_ipv4() == r.gateway.is_ipv4()
                    && !r.gateway.is_unspecified()
                    && !r.gateway.is_multicast(),
                "Invalid route interface or gateway family"
            );
        }
        let mut pools = Vec::new();
        for d in &self.dhcp {
            ensure!(
                names.contains(d.interface.as_str()) && d.subnet.addr().is_ipv4(),
                "DHCPv4 requires a declared interface and IPv4 subnet"
            );
            ensure!(
                d.subnet.contains(&d.start)
                    && d.subnet.contains(&d.end)
                    && d.subnet.contains(&d.router),
                "DHCP range and router must belong to its subnet"
            );
            let (IpAddr::V4(start), IpAddr::V4(end), IpAddr::V4(router)) =
                (d.start, d.end, d.router)
            else {
                anyhow::bail!("DHCPv4 requires IPv4 addresses")
            };
            let (start, end, router) = (u32::from(start), u32::from(end), u32::from(router));
            ensure!(
                start <= end && end - start < 65536 && (router < start || router > end),
                "Invalid DHCP range or router inside the allocation pool"
            );
            ensure!(
                !pools.iter().any(|(a, b)| start <= *b && end >= *a),
                "DHCP pools overlap"
            );
            pools.push((start, end));
            ensure!(
                !addresses
                    .iter()
                    .any(|a| matches!(a,IpAddr::V4(v) if (start..=end).contains(&u32::from(*v)))),
                "DHCP pool overlaps a statically assigned address"
            );
        }
        if let Some(r) = &self.resolver {
            ensure!(
                !r.listen.is_empty()
                    && !r.allow.is_empty()
                    && r.listen.len() <= 16
                    && r.allow.len() <= 64,
                "Resolver requires bounded listen and client allow lists"
            );
            ensure!(
                r.listen
                    .iter()
                    .all(|i| !i.is_unspecified() && !i.is_multicast()),
                "An open resolver must not listen on an unspecified address"
            );
        }
        let mut zones = HashSet::new();
        for z in &self.zones {
            ensure!(
                dns_name(&z.name)
                    && dns_name(&z.primary_ns)
                    && dns_name(&z.admin_mailbox)
                    && zones.insert(z.name.to_lowercase())
                    && z.records.len() <= 4096,
                "Invalid or duplicate DNS zone"
            );
            for r in &z.records {
                ensure!(
                    r.name == "@" || dns_name(&r.name),
                    "Invalid DNS record name"
                );
                ensure!(
                    r.value.len() <= 255 && !r.value.chars().any(char::is_control),
                    "Invalid DNS record value"
                );
                match r.kind {
                    RecordType::A => {
                        r.value.parse::<Ipv4Addr>()?;
                    }
                    RecordType::AAAA => {
                        r.value.parse::<std::net::Ipv6Addr>()?;
                    }
                    RecordType::CNAME | RecordType::MX => {
                        ensure!(dns_name(&r.value), "Invalid target DNS name")
                    }
                    RecordType::TXT => {}
                }
            }
        }
        if let Some(b) = &self.bgp {
            ensure!(
                b.asn > 0
                    && !b.router_id.is_unspecified()
                    && !b.peers.is_empty()
                    && b.peers.len() <= 64
                    && b.announce.len() <= 256,
                "Invalid BGP declaration"
            );
            for p in &b.peers {
                ensure!(
                    p.asn > 0 && !p.address.is_unspecified() && !p.address.is_multicast(),
                    "Invalid BGP peer"
                );
            }
        }
        for r in &self.firewall {
            ensure!(
                names.contains(r.interface.as_str()) && r.port > 0,
                "Firewall rule requires a declared interface and valid port"
            );
        }
        for n in &self.nat {
            ensure!(name(&n.egress), "Invalid NAT egress interface");
        }
        Ok(())
    }
    pub fn plan(&self) -> Result<Plan> {
        self.validate()?;
        let mut files = BTreeMap::new();
        let mut packages = std::collections::BTreeSet::from(["systemd-networkd", "iproute2"]);
        let mut review = vec![
            "Import and review existing network ownership before live activation".into(),
            "Management connectivity and timed rollback must be tested before host changes".into(),
        ];
        for i in &self.interfaces {
            if !matches!(i.link, Link::Physical) {
                let mut text = format!(
                    "[NetDev]\nName={}\nKind={}\nMTUBytes={}\n",
                    i.name,
                    match i.link {
                        Link::Vlan { .. } => "vlan",
                        Link::Bridge { .. } => "bridge",
                        Link::Bond { .. } => "bond",
                        _ => unreachable!(),
                    },
                    i.mtu
                );
                match &i.link {
                    Link::Vlan { tag, .. } => text += &format!("\n[VLAN]\nId={tag}\n"),
                    Link::Bond { mode, .. } => {
                        text += &format!(
                            "\n[Bond]\nMode={}\n",
                            match mode {
                                BondMode::ActiveBackup => "active-backup",
                                BondMode::Ieee8023ad => "802.3ad",
                            }
                        )
                    }
                    _ => {}
                }
                files.insert(
                    format!("etc/systemd/network/70-kagnode-{}.netdev", i.name),
                    text,
                );
            }
            let mut text = format!(
                "[Match]\nName={}\n\n[Link]\nMTUBytes={}\n\n[Network]\nDHCP={}\n",
                i.name,
                i.mtu,
                if i.dhcp { "yes" } else { "no" }
            );
            for a in &i.addresses {
                text += &format!("Address={a}\n");
            }
            for d in &self.dns_servers {
                text += &format!("DNS={d}\n");
            }
            for child in &self.interfaces {
                if let Link::Vlan { parent, .. } = &child.link
                    && parent == &i.name
                {
                    text += &format!("VLAN={}\n", child.name);
                }
                if let Link::Bridge { ports } | Link::Bond { ports, .. } = &child.link
                    && ports.contains(&i.name)
                {
                    text += &format!(
                        "{}={}\n",
                        if matches!(child.link, Link::Bridge { .. }) {
                            "Bridge"
                        } else {
                            "Bond"
                        },
                        child.name
                    );
                }
            }
            for r in self.routes.iter().filter(|r| r.interface == i.name) {
                text += &format!(
                    "\n[Route]\nDestination={}\nGateway={}\nMetric={}\n",
                    r.destination, r.gateway, r.metric
                );
            }
            files.insert(
                format!("etc/systemd/network/70-kagnode-{}.network", i.name),
                text,
            );
            if let Link::Vlan { parent, tag } = &i.link {
                review.push(format!("Verify VLAN {tag} is carried by {parent} and every intervening bridge/switch trunk"));
            }
        }
        if !self.dhcp.is_empty() {
            packages.insert("kea-dhcp4-server");
            let subnets:Vec<_>=self.dhcp.iter().enumerate().map(|(index,d)|json!({"id":index+1,"subnet":d.subnet,"interface":d.interface,"pools":[{"pool":format!("{} - {}",d.start,d.end)}],"option-data":[{"name":"routers","data":d.router.to_string()},{"name":"domain-name-servers","data":d.dns.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")}]})).collect();
            let config = json!({"Dhcp4":{"interfaces-config":{"interfaces":self.dhcp.iter().map(|d|&d.interface).collect::<Vec<_>>()},"lease-database":{"type":"memfile","persist":true,"name":"/var/lib/kagnode-netctl/kea-leases4.csv"},"valid-lifetime":3600,"subnet4":subnets}});
            files.insert(
                "etc/kagnode/netctl/kea-dhcp4.json".into(),
                serde_json::to_string_pretty(&config)?,
            );
            review.push(
                "Refuse activation while another DHCP server owns the selected interfaces".into(),
            );
        }
        if let Some(r) = &self.resolver {
            packages.insert("unbound");
            let mut text="server:\n  verbosity: 1\n  hide-identity: yes\n  hide-version: yes\n  access-control: 0.0.0.0/0 refuse\n  access-control: ::/0 refuse\n".to_owned();
            for ip in &r.listen {
                text += &format!("  interface: {ip}\n");
            }
            for net in &r.allow {
                text += &format!("  access-control: {net} allow\n");
            }
            if !r.forwarders.is_empty() {
                text += "forward-zone:\n  name: \".\"\n";
                for ip in &r.forwarders {
                    text += &format!("  forward-addr: {ip}\n");
                }
            }
            files.insert("etc/kagnode/netctl/unbound.conf".into(), text);
        }
        if !self.zones.is_empty() {
            packages.insert("knot");
            let mut text = "server:\n  rundir: /run/kagnode-netctl/knot\nzone:\n".to_owned();
            for z in &self.zones {
                text += &format!(
                    "  - domain: {}\n    file: /etc/kagnode/netctl/zones/{}.zone\n",
                    z.name, z.name
                );
                let mut zone = format!(
                    "$ORIGIN {}.\n$TTL 300\n@ IN SOA {}. {}. (1 3600 900 1209600 300)\n@ IN NS {}.\n",
                    z.name.trim_end_matches('.'),
                    z.primary_ns.trim_end_matches('.'),
                    z.admin_mailbox.trim_end_matches('.'),
                    z.primary_ns.trim_end_matches('.')
                );
                for r in &z.records {
                    let value = match r.kind {
                        RecordType::TXT => {
                            format!("\"{}\"", r.value.replace('\\', "\\\\").replace('"', "\\\""))
                        }
                        RecordType::MX => {
                            format!("{} {}.", r.priority, r.value.trim_end_matches('.'))
                        }
                        RecordType::CNAME => format!("{}.", r.value.trim_end_matches('.')),
                        _ => r.value.clone(),
                    };
                    zone += &format!(
                        "{} IN {} {value}\n",
                        r.name,
                        match r.kind {
                            RecordType::A => "A",
                            RecordType::AAAA => "AAAA",
                            RecordType::CNAME => "CNAME",
                            RecordType::MX => "MX",
                            RecordType::TXT => "TXT",
                        }
                    );
                }
                files.insert(format!("etc/kagnode/netctl/zones/{}.zone", z.name), zone);
            }
            files.insert("etc/kagnode/netctl/knot.conf".into(), text);
            review.push("Declare authoritative listen addresses and reconcile DNS port ownership before activation".into());
        }
        if let Some(b) = &self.bgp {
            packages.insert("frr");
            let mut text = format!(
                "frr defaults traditional\nhostname kagnode-netctl\nrouter bgp {}\n bgp router-id {}\n",
                b.asn, b.router_id
            );
            for peer in &b.peers {
                text += &format!(" neighbor {} remote-as {}\n", peer.address, peer.asn);
            }
            for family in [true, false] {
                let announce: Vec<_> = b
                    .announce
                    .iter()
                    .filter(|n| n.addr().is_ipv4() == family)
                    .collect();
                if !announce.is_empty() {
                    text += if family {
                        " address-family ipv4 unicast\n"
                    } else {
                        " address-family ipv6 unicast\n"
                    };
                    for net in announce {
                        text += &format!("  network {net}\n");
                    }
                    text += " exit-address-family\n";
                }
            }
            files.insert("etc/kagnode/netctl/frr.conf".into(), text);
            review.push(
                "Review BGP peer policies and announcement filters before establishing sessions"
                    .into(),
            );
        }
        if !self.firewall.is_empty() || !self.nat.is_empty() {
            packages.insert("nftables");
            let mut text="table inet kagnode_netctl {\n chain input {\n  type filter hook input priority 10; policy accept;\n".to_owned();
            for r in &self.firewall {
                text += &format!(
                    "  iifname \"{}\" {} saddr {} {} dport {} {}\n",
                    r.interface,
                    if r.source.addr().is_ipv4() {
                        "ip"
                    } else {
                        "ip6"
                    },
                    r.source,
                    match r.protocol {
                        Protocol::Tcp => "tcp",
                        Protocol::Udp => "udp",
                    },
                    r.port,
                    if r.accept { "accept" } else { "drop" }
                );
            }
            text += " }\n chain postrouting {\n  type nat hook postrouting priority 100; policy accept;\n";
            for n in &self.nat {
                text += &format!(
                    "  {} saddr {} oifname \"{}\" masquerade\n",
                    if n.source.addr().is_ipv4() {
                        "ip"
                    } else {
                        "ip6"
                    },
                    n.source,
                    n.egress
                );
            }
            text += " }\n}\n";
            files.insert("etc/kagnode/netctl/firewall.nft".into(), text);
            review.push("Keep existing firewall tables; validate the owned table in isolation before replacement".into());
        }
        let hash = hex::encode(Sha256::digest(serde_json::to_vec(&files)?));
        Ok(Plan {
            schema: 1,
            sha256: hash,
            files,
            prerequisites: packages.into_iter().map(str::to_owned).collect(),
            review,
            live_activation: false,
        })
    }
}
impl Plan {
    pub fn write_new(&self, output: &Path) -> Result<()> {
        ensure!(
            !output.exists(),
            "Output already exists; do not overwrite an unreviewed configuration"
        );
        for parent in output.ancestors().filter(|p| p.exists()) {
            ensure!(!parent.is_symlink(), "Output ancestors cannot be symlinks");
        }
        std::fs::create_dir_all(output)?;
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(output, std::fs::Permissions::from_mode(0o700))?;
        for (name, data) in &self.files {
            let path = std::path::Path::new(name);
            ensure!(
                !path.is_absolute()
                    && !path
                        .components()
                        .any(|p| !matches!(p, std::path::Component::Normal(_))),
                "Unsafe generated path"
            );
            let destination = output.join(path);
            std::fs::create_dir_all(destination.parent().unwrap())?;
            std::fs::write(&destination, data)?;
            std::fs::set_permissions(destination, std::fs::Permissions::from_mode(0o600))?;
        }
        std::fs::write(output.join("plan.json"), serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }
}
pub fn capability() -> Value {
    json!({"schema":1,"product":"kagnode-netctl","version":env!("CARGO_PKG_VERSION"),"inspect":true,"render":["networkd","kea-dhcp4","unbound","knot","frr-bgp","nftables"],"ipam":true,"physical_device_writes":false,"live_network_activation":false,"ha_vlan_requires_observed_connectivity":true})
}
