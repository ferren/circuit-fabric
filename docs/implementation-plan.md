# CircuitFabric Implementation Plan

[简体中文](implementation-plan.zh-CN.md)

## Delivery strategy

Build the deterministic semantic and verification foundation before expanding EDA writes or adding more agent runtimes. Every phase must preserve a usable read-only path and must not claim a write succeeded only because a backend API returned success.

## Phase 0 — Contracts and repository foundation

Deliver:

- versioned schemas for Logical Circuit IR, patches, diffs, evidence, constraints, materialization plans/results, and verification reports;
- declarative plugin manifest schema for agent runtimes and EDA backends;
- repository tooling, test fixture policy, formatting, and secret-safe local configuration.

Acceptance:

- contract examples validate and remain backward-versioned;
- plugin discovery validates manifests without executing plugin code;
- no secret or generated state is tracked by Git.

## Phase 1 — Immutable snapshot store and deterministic validation

Deliver:

- content-addressed snapshot store with parent lineage;
- canonical serialization and separate `logicalHash` / `physicalHash`;
- Schema, identity, and topology validators;
- IR diff generation and provenance-aware evidence references.

Acceptance:

- importing the same fixture produces the same snapshot hash;
- physical-only edits preserve `logicalHash`;
- invalid pin/net references fail deterministically;
- a missing observation produces `inconclusive`, never `passed`.

## Phase 2 — Hardware Document Service

Deliver:

- authorized source registration and safe scanner;
- PDF/text/BOM/netlist ingestion with content hashes and citations;
- source-aware keyword retrieval and evidence packages;
- document-version and source-trust metadata.
- project-scoped document retrieval for EDA-originated agent sessions, including persisted citations in session audit records.

Acceptance:

- agents can retrieve only indexed authorized content;
- every returned electrical parameter includes a document locator and content hash;
- document updates invalidate derived evidence packages predictably.
- an EDA session can use only its project's authorized documents, and every document-derived response retains its source locator.

## Phase 3 — JLCircuit observed-design importer

Deliver:

- a read-only JLCircuit bridge plugin;
- an EDA-local bridge panel for conversation, current-design context capture, task progress, and project-scoped document retrieval;
- component, pin, wire, net, bus, document, DRC/ERC, and canvas observation import;
- external-object identity mapping and capability reporting;
- fixture matrix for API/version differences.

Acceptance:

- a fixed JLCircuit schematic yields a stable observed snapshot;
- incomplete API fields remain explicitly unknown/inconclusive;
- imported primitive IDs are external references, not logical identifiers;
- screenshots remain evidence, not a substitute for topology facts.
- the bridge stores no API key and delegates runtime, MCP/skill, and document policy to CircuitFabric.

## Phase 4 — Fusion, ChangeSets, and safe materialization

Deliver:

- baseline/target/readback reconciliation;
- semantic IR patches converted into reviewed ChangeSets;
- JLCircuit `preview`, `apply`, `readback`, and compensation/rollback implementation;
- precondition checks and structured verification report.

Acceptance:

- design drift rejects a stale plan;
- partial failure stops remaining unsafe work and reports compensation outcome;
- write success, readback success, and verification success remain separately visible;
- known bridge-read latency can be `inconclusive` without falsely rolling back confirmed writes.

## Phase 5 — External agent runtime host

Deliver:

- Codex App Server adapter with streaming, dynamic tools, token usage, cancellation, and thread mapping;
- desktop control-plane workflows for projects, documents, EDA bridge health, runtime/API-key references, MCP/skill grants, session audit, task status, usage, semantic queries, and BOM export;
- runtime-neutral task event protocol and bounded tool context;
- provider configuration isolated per process and per project;
- stub conformance fixtures for Claude Code/DSH adapters.

Acceptance:

- an agent can inspect documents and observed IR, propose an IRPatch, and request permitted tools;
- agent text cannot bypass approval or materialization policy;
- runtime restart can recover a task using only stored references and snapshots.

## Phase 6 — Additional EDA and engineering plugins

Deliver:

- second backend proof of portability, preferably KiCad read/import first;
- BOM, netlist, and SPICE export/import plugins;
- plugin lifecycle, signature/trust policy, health reporting, permissions, and isolation.

Acceptance:

- equivalent logical circuit imports from two backends reconcile to comparable topology;
- a plugin cannot access another plugin's credentials or arbitrary project files;
- every capability is declared, granted, audited, and revocable.

## Phase 7 — Product workflows and evaluation

Deliver:

- review UI for IR diffs, document evidence, ChangeSets, verification, and rollback;
- reusable reference-circuit and datasheet-review workflows;
- fixture corpus and evaluations for topology correctness, document citation quality, write safety, and readability.

Acceptance:

- users can understand planned changes without reading raw model reasoning;
- regressions are measured against fixed electrical and visual test designs;
- unsupported claims and unverifiable writes are surfaced clearly.
