# CircuitFabric

[简体中文](README.zh-CN.md)

CircuitFabric is an AI-native circuit design platform with a semantic core, hardware-document evidence, and pluggable EDA materialization backends.

It is not an assistant embedded in a single EDA product. CircuitFabric is the controlled layer between external agent runtimes, such as Codex App Server, and engineering tools such as JLCircuit EDA, KiCad, netlist exporters, BOM systems, and simulators.

```text
External Agent Runtime
        |
        v
Circuit Semantic Core / Logical Circuit IR
        |
        v
Fusion and Materialization Layer
        |
        v
EDA, netlist, BOM, simulation, and verification plugins
```

## Scope

- Keep logical circuit facts, semantic intent, constraints, evidence, and change history independent of any EDA vendor.
- Make hardware documents—datasheets, reference circuits, BOMs, and design notes—first-class, source-traceable inputs.
- Make a project's authorized documents available to an EDA-originated agent session through source-aware retrieval, so every document-derived claim can be cited.
- Let external agents propose plans and patches without granting them direct access to raw EDA APIs or persistent design state.
- Materialize approved changes through versioned EDA backend plugins and verify them through readback, DRC/ERC, visual evidence, or simulation.

## Repository layout

- `crates/`: domain contracts and deterministic services; none depends on a particular EDA or agent runtime.
- `plugins/`: independently versioned adapters, starting with the JLCircuit EDA bridge contract.
- `apps/`: product shells, starting with the GPUI desktop control plane.
- `assets/branding/`: project-owned application artwork, including the multi-size Windows icon resource.
- [Architecture](docs/architecture.md) / [架构](docs/architecture.zh-CN.md): target architecture, ownership, constraints, and plugin boundaries.
- [Implementation plan](docs/implementation-plan.md) / [实施计划](docs/implementation-plan.zh-CN.md): phased delivery plan and acceptance criteria.
- [Provider settings](docs/provider-settings.md): multi-provider configuration and credential boundary.
- [TODO](TODO.md): implementation backlog in execution order.

## Status

This repository is intentionally starting as a clean architecture and contract baseline. The first implementation milestone is a deterministic Logical Circuit IR snapshot store plus a JLCircuit EDA observed-design importer. The JLCircuit EDA bridge will be an independent implementation in this repository: JLCircuit-Agent is behavioral reference material only, never a build, runtime, or source-code dependency.

## Quick start

Install Rust 1.88 or newer, then run the deterministic workspace checks:

```powershell
cargo fmt --all -- --check
cargo test --workspace --exclude circuitfabric-desktop
cargo clippy --workspace --exclude circuitfabric-desktop --all-targets -- -D warnings
```

The desktop crate is a GPUI control-plane scaffold with a multi-provider manager. Build its native UI with `--features native-ui` after the pinned GPUI dependencies are available. Provider settings are saved under the platform application-data directory and contain API-key environment-variable names only. The JLCircuit extension package can be rebuilt with `./scripts/package-jlc-extension.ps1`; see [`artifacts/README.md`](artifacts/README.md).

## Core principles

1. The agent proposes; the semantic core owns facts.
2. Logical identity is independent of EDA primitive IDs and physical layout.
3. Every actionable change is snapshot-addressed, reviewable, auditable, and verifiable.
4. A backend write that cannot be read back is `inconclusive`, not successful.
5. Plugins never bypass the semantic core, user approval, or verification policy.
