# Network review and activation boundary

Rust validates a declaration, renders an owned file set and records a deterministic SHA-256. Discovery is read-only. IPAM is persistent administration state and is not proof that an address is free on a physical network.

The current configuration backend is networkd. Existing NetworkManager, ifupdown and Proxmox ownership must be imported and reconciled before host application. The preview cannot take over those interfaces. VLAN/bond declarations require matching switch/trunk/LACP configuration; discovery cannot prove that configuration by reading one host.

Network service roles are explicit: Kea DHCPv4, Unbound recursive DNS, Knot authoritative DNS, FRR BGP and nftables. Existing services keep their ports and configuration. Conflict detection, zone-serial management, route policies, timed transaction rollback and service-specific validation are release gates for live activation. DHCPv6/RA, additional dynamic routing protocols and physical network-device writes are outside the initial renderer.

Daemon uses KagNode. Network readiness when reviewing a same-site HA network. Host, witness, fencing and synchronous-storage evidence remain separate requirements. A planned VLAN never authorizes promotion. Cluster and storage ownership belong to the qualified Pacemaker/Corosync/DRBD adapters, not this configuration renderer.
