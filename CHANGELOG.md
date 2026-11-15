# Changelog

## Unreleased

- Rename the visible product name from KagNode. Netctl to KagNode. Network in the interface, command help, documentation and artwork text. The repository, crate, `kagnode-netctl` executable, configuration and state paths, JSON product identifier and published history keep their identifiers.

## 5.0.0

- Add Rust host inventory, typed network declarations and deterministic configuration plans.
- Render bounded networkd, DHCPv4, recursive/authoritative DNS, BGP and firewall/NAT configuration.
- Add transactional IPv4/IPv6 IPAM with persistent, idempotent allocation.
- Keep live activation gated pending networking, recovery and application qualification.
