<p align="center"><img src="assets/mark.svg" width="72" alt="KagNode."></p>

# KagNode. Netctl

## Overview

Rust tooling for inspected, reviewable Linux networking and network services. Netctl is part of the KagNode Daemon ecosystem.

### What it looks like

A plan run from the repository's VLAN example, followed by an IP allocation, in a throwaway directory. Nothing on the host is changed. The addresses are documentation examples.

![Terminal output of a network plan with its review notes and checksum, then an IP pool and one allocation](docs/images/plan-and-ipam.png)

### How it works

1. **Declare.** You describe the wanted network (VLANs, bridges, bonds, addresses, routes, DNS) in typed JSON. Unknown fields are rejected.
2. **Plan.** `plan` validates the declaration and writes a fresh directory with the configuration files for systemd-networkd, Kea, Unbound, Knot DNS, FRR and an owned nftables table, plus `plan.json` with a checksum, prerequisites and review notes.
3. **Review.** The plan states `live_activation: false` and lists what a person must check first, such as whether a switch trunk carries the VLAN. The same declaration always produces the same plan hash.
4. **Nothing is applied for you.** Netctl only reads the host and writes files into a new directory. Applying them needs a qualified activation adapter, console access and a timed rollback, which are not available yet.
5. **IPAM.** Address pools and allocations are kept in a private SQLite file. An allocation is unique and repeatable for the same owner, and releasing one is an explicit step.

## Release status

**5.0.0 source preview.** Host inspection, typed configuration planning and persistent IPAM have automated coverage. Plans render owned configuration files; live host activation, automatic HA, physical-switch changes and Proxmox writes remain blocked until their transaction and recovery adapters are qualified. A generated plan is not a successful network change.

## Features

- Bounded read-only interface, address, route and service inventory.
- Validated VLAN, bridge, bond, address, route and DNS-client declarations.
- Configuration output for systemd-networkd, Kea DHCPv4, Unbound, Knot DNS, FRR BGP and an owned nftables table.
- Persistent IPv4/IPv6 IPAM allocation windows with unique, idempotent allocation and explicit release.
- Deterministic plan hashes, fresh output directories, input bounds and source-visible validation.
- HA VLAN/trunk requirements appear in the plan. Netctl does not assume that creating a VLAN interface configures intervening switches.

## Requirements

Rust 1.88 or newer for development. Linux inspection needs `iproute2` and systemd. Rendered service configurations list their dependencies; rendering does not install them. Native service engines retain their own licenses and verification requirements.

## Installation and downloads

Build with `cargo build --release --locked`. The executable is `target/release/kagnode-netctl`.

| Payload                               | Formats                                        | Qualification                                     |
| ------------------------------------- | ---------------------------------------------- | ------------------------------------------------- |
| Editable source                       | Git checkout; GitHub source `.zip` / `.tar.gz` | Preview; see validation scope                     |
| Application and distribution packages | `.deb`, `.rpm`, `.tar.xz`, `.tar.gz`, `.zip`   | Release packaging pending native lifecycle checks |

Use the published source rather than a third-party install script. Package installation will not silently configure networking.

## Usage

```sh
kagnode-netctl capabilities
kagnode-netctl inspect
kagnode-netctl plan --file docs/vlan-example.json --output ./reviewed-network
kagnode-netctl ipam --root /srv/kagnode/netctl add-pool --name ha --subnet 10.42.0.0/24 --start 10.42.0.10 --end 10.42.0.50
kagnode-netctl ipam --root /srv/kagnode/netctl allocate --pool ha --owner primary
```

The output directory must be new. Review `plan.json`, its checksums, requirements and generated files. Ordinary users must not install generated files into `/etc` as an unreviewed shortcut around activation gates.

## Configuration and storage

Declarations use typed JSON with unknown fields rejected. IPAM stores private SQLite state beneath the explicitly selected root, default `/srv/kagnode/netctl`. Lease ownership is separate from a machine's live DHCP leases. Existing network managers and configurations retain ownership.

The example demonstrates a physical parent and HA VLAN. It does not verify a switch trunk, fencing, witness availability or durable replication.

## Security

All external commands have fixed read-only argument arrays, time/output bounds and no shell interpolation. Generated names and paths are validated. Output does not overwrite existing configuration. DNS record values cannot inject configuration lines. IPAM uses transactions and uniqueness constraints. Keep private state, credentials and production configuration outside Git.

Network changes can remove management access. Native activation requires a qualified adapter, independent console access and timed rollback. Physical router/switch configuration is outside this preview.

## Development and testing

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

Tests cover declaration validation, deterministic rendering, overwrite/symlink refusal, DNS escaping, DHCP conflicts, persistent/idempotent IP allocation and overlapping windows. No test result here establishes production routing or HA readiness.

## Documentation

- [Architecture and validation boundaries](docs/ARCHITECTURE.md)
- [VLAN declaration](docs/vlan-example.json)
- [Repository layout](docs/STRUCTURE.md)
- [Changelog](CHANGELOG.md)

## Licensing

Code: **AGPL-3.0-only**, see [LICENSE](LICENSE). Artwork follows its [separate notice](assets/ARTWORK-LICENSE). External network-service engines keep their existing licenses.
