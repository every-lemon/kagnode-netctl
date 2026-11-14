# Repository layout

- `src/lib.rs`: declaration types, validation and configuration rendering.
- `src/observe.rs`: bounded read-only operating-system inventory.
- `src/ipam.rs`: private transactional IP address allocation.
- `src/main.rs`: scriptable Rust command interface.
- `tests/`: isolated regression tests; no production network mutation.
- `assets/`: approved artwork and its separate license.
- `packaging/`: release metadata and explicit committed-source inventory.
- `.github/workflows/`: non-mutating formatting, static and test checks.

`target/`, runtime databases, lease state, private credentials and generated review directories are not published source. Generated service configuration belongs in a reviewed output directory, never alongside canonical editable code.
