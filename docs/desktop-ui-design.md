# CircuitFabric Desktop Control-Plane UI Design

[简体中文](desktop-ui-design.zh-CN.md)

> This file is the UI design baseline for the desktop control plane (`apps/circuitfabric-desktop`, GPUI + `gpui-component`). It answers two questions: **how the interface is categorized** (information architecture) and **what it looks like** (visuals and layout). Each page's `[TODO]` marks the "entry point planned, concrete function not yet implemented" parts — navigation and layout land first, function follows.

## 1. Positioning and principles

The desktop control plane is an **engineering console**, not a second schematic editor and not a second chat client. It owns projects, authorized documents, EDA service/bridge health, runtime endpoints and API-key references, skills/MCP authorization, sessions/tasks/audit, usage, semantic query, and BOM export. Schematic editing and in-EDA conversation belong to each backend/bridge plugin.

Principles (applied across the whole app):

1. **Categorize by workflow, not by technical layer.** Users navigate "create project → authorize documents → connect EDA → configure agent → run session → review change → export", not "store/fusion/materialization" layers.
2. **Observable state, never disguised.** Write success, readback success, and verification success are three independent states; `inconclusive` (readback failed) is always amber for "unknown", never green "success".
3. **Safety first.** High-risk materialization requires explicit approval bound to a baseline snapshot + plan hash; API keys appear only as **environment-variable names** — no key value is ever stored or displayed.
4. **Modern and beautiful.** Light/dark/system themes, unified color/type/spacing tokens, rounded cards, restrained shadows and motion, consistent empty states.
5. **Progressive delivery.** Unimplemented functions are shown with a `TODO` badge; entries are clickable into a "coming soon" placeholder that lists the target phase.

## 2. Information architecture (navigation categories)

Classic "sidebar + top bar + content + status bar" frame. Top-level navigation grouped by workflow:

```text
CircuitFabric
│
├─ Overview         仪表盘: project/document/bridge/task/pending-approval health
│
├─ Projects         project CRUD + project-level config entry
├─ Documents        authorized hardware document registration / retrieval / evidence
├─ Semantics        snapshot lineage / topology / constraints / semantic query
│
├─ EDA Services     backend/bridge plugin health and connection
├─ Agents & Tools   runtime endpoints / LLM providers / skills & MCP authorization
├─ Sessions         session replay / task progress / event trace
│
├─ Changes          ChangeSet / IR diff / verification report / rollback
├─ BOM & Export     BOM export / netlist / SPICE
│
├─ Plugins          manifest / version lock / signature / permissions / health
├─ Usage & Audit    token usage / audit log
│
└─ Settings         theme / language / data dir / secret store / about
```

Group semantics:

- **Design & Evidence** (Overview, Projects, Documents, Semantics): where a project's design facts come from and what they are now.
- **Run & Tools** (EDA Services, Agents & Tools, Sessions): who changes things, through which tools, and session progress.
- **Materialize & Deliver** (Changes, BOM & Export): how changes are reviewed, landed to EDA, and exported.
- **Governance** (Plugins, Usage & Audit, Settings): are plugins trusted, how much was spent, global config.

### Mapping to existing code

The current `ControlPlaneScreen` enum in `main.rs` has 7 screens. This design extends it (5 new screens) and keeps existing names so migration is mechanical:

| Existing enum item | This design's module | Change |
| --- | --- | --- |
| `Projects` | Projects | kept, adds detail tabs |
| `Documents` | Documents | kept, adds retrieval & evidence |
| `EdaServices` | EDA Services | kept, adds capability badges & health |
| `AgentsAndMcp` | Agents & Tools | kept, existing provider form moves into a subpage |
| `SessionsAndTasks` | Sessions | kept, adds replay & trace |
| `Usage` | Usage & Audit | kept, merges audit |
| `SemanticQueryAndBom` | Semantics + BOM & Export | split into two screens |
| — (new) | Overview / Changes / Plugins / Settings | new |

## 3. Global frame (shell)

### 3.1 Top bar

```text
┌─ [☰] ─ [project selector ▾] ────────────── [global search / palette ⌘K] ── [●run] [provider] [🌙] [🔔] [⚙] ─┐
```

- **Left**: sidebar collapse button; project selector (dropdown showing project name + status dot; content follows project context).
- **Middle**: global search box; focus opens the command palette (`Ctrl+K`) to search projects / documents / sessions / commands.
- **Right**: runtime status indicator (Codex App Server / bridge green when online, gray offline), default provider badge, theme toggle, notifications, settings.

### 3.2 Sidebar

- Group headers + line icons + text; icons only when collapsed.
- Unimplemented modules show a small `TODO` badge (gray dot) next to the icon.
- Bottom fixed: settings, help, version.

### 3.3 Status bar

- Left: bridge address and health, default provider, last verification status (reusing the status colors of §4.2).
- Right: version, data directory path, open logs.

### 3.4 Content area

Content follows one of three shapes: **list + detail** (projects, documents, sessions, changes, plugins), **form** (agents & tools, settings), **dashboard** (overview, usage).

### 3.5 Approval drawer

High-risk materialization approval uses a right-side drawer, not a modal; see §5.8.

## 4. Visual design system (design tokens)

### 4.1 Theme

Light / dark / follow system, three-way toggle. All colors use semantic tokens, no scattered hardcoded values.

### 4.2 Status colors (core semantics)

Map the contract's `FactStatus` directly to site-wide status colors, so "write/readback/verify" semantics stay consistent:

| `FactStatus` | Color | Meaning |
| --- | --- | --- |
| `passed` | green | passed |
| `failed` | red | failed |
| `inconclusive` | amber | unknown / readback failed (**never shown as success**) |
| `not_run` | gray | not run |

Plus neutral tokens: `surface`, `surface-muted`, `canvas`, `text` / `text-muted` / `text-faint`, `border` / `border-strong`, and brand `accent` (the circuit green/blue of the logo).

### 4.3 Type

- Ranks: `display` / `title` / `heading` / `body` / `caption` / `mono`.
- `mono` for every machine identifier: snapshot hash, provider id, content hash, citation locator, command.

### 4.4 Spacing and radius

- Spacing on a 4px grid (`4/8/12/16/24/32`).
- Radius: `sm 6` / `md 8` / `lg 12` / `xl 16`.

### 4.5 Icons

One line icon per module (overview=grid, projects=folder, documents=page, semantics=topology nodes, EDA services=plug, agents=bot, sessions=bubble, changes=branch, BOM=checklist, plugins=puzzle, usage=bar chart, settings=gear).

### 4.6 Base components

`Button` (primary / secondary / ghost / danger), `Card`, `List`, `Table`, `Tabs`, `SegmentedControl`, `Badge`/`Tag` (status badge, `TODO` badge), `Modal`, `Drawer`, `Toast`, `EmptyState`, `CommandPalette`, `StatusDot`.

## 5. Per-screen design

### 5.1 Overview

Dashboard. Four KPI cards on top: project count, authorized document count, online bridge count, pending approval count. Below, three columns:

- **Runtime health**: Codex App Server and JLC bridge connection cards (status dot + address + last heartbeat).
- **Recent sessions/tasks**: timeline (last 5, with project, runtime, status).
- **To-do**: pending ChangeSets, unresolved identity conflicts, `inconclusive` verification results.

```text
┌ projects 3 ┐ ┌ docs 12 ┐ ┌ online bridge 1 ┐ ┌ pending 2 ┐
┌─ runtime health ─────────┐ ┌─ recent sessions ─────┐ ┌─ to-do ──────────┐
│ ● Codex App Server online │ │ ● s_01 projA done      │ │ ! ChangeSet #12  │
│ ● JLC bridge 127.0.0.1    │ │ ○ s_02 projA running   │ │ ! conflicts x2   │
│ ○ no bridge-ui session    │ │ ...                   │ │ △ readback incon.│
└──────────────────────────┘ └────────────────────────┘ └──────────────────┘
```

`[TODO]` chart components, usage aggregation, heartbeat push.

### 5.2 Projects

Two panes: project list on the left (search + filter), detail on the right.

- Project card: name, description, document count, session count, last activity.
- Detail tabs: **Overview / Documents / Sessions / Agent config / Usage**.
- New project: modal form (ID + name + description) with instant unique-ID validation.

`[TODO]` project-scoped config persistence, archive/delete confirmations.

### 5.3 Documents

Authorized document library.

- List columns: title, kind (PDF/Word/Markdown/BOM/Netlist/Text), source locator, content hash (mono, truncated), version, trust, status (indexed / pending scan).
- Top bar: **register source** (choose file or enter locator) → trigger security scan.
- Detail: metadata + extracted fragments + citation locators.
- **Source-aware retrieval**: enter query → return fragments (with `document_id` + content hash + locator), demonstrating "every claim is citable".

`[TODO]` PDF/Word parsing and fragment extraction, security scanner, version-update evidence-package invalidation cascade.

### 5.4 Semantics

- **Snapshot lineage**: tree/timeline, each snapshot tagged with authority (`observed` / `planned` / `verified`), showing `logicalHash` and `physicalHash` side by side (mono).
- **Topology**: component / pin / net lists (table now, diagram later).
- **Constraints & validation**: colored by §4.2 status, with evidence references.
- **Semantic query**: structured query (filter by component/net/constraint).

`[TODO]` topology rendering, IR diff visualization, diff generation.

### 5.5 EDA Services

Backend/bridge plugin list. Each card: kind, version, `capabilities` badges (`inspect` / `preview` / `apply` / `readback` / `rollback` / `drc` / `visual-capture` / `bridge-chat` / `bridge-context`), address, health, last readback status.

```text
┌ JLCircuit EDA backend ────────────────────────────── [v0.1] ● online ┐
│ transport bridge · ws://127.0.0.1:49630/bridge                     │
│ [inspect] [preview] [apply] [readback] [rollback] [drc] [visual] … │
│ last readback: inconclusive (amber)                                │
└─────────────────────────────────────────────────────────────────────┘
```

`[TODO]` heartbeat & health reporting, capability reporting, connection test.

Implemented today: the page owns the JLCircuit bridge **listen address** (loopback-only, persisted into the runtime settings the manually-launched `circuitfabric-jlc-bridge` reads) — the bridge is an EDA-side transport, so its endpoint is configured here, never on the agent runtime endpoints.

### 5.6 Agents & Tools

Two-pane layout mirroring Projects (grouped list on the left, detail on the right):

- **Runtime endpoints**: Codex App Server command / working directory with a supervised start & stop lifecycle (status chip: starting / running · PID / stopped / failed, echoed in the sidebar); the JLC bridge listen address belongs to EDA Services (§5.5); Claude Code and DSH shown as "coming soon" stubs.
- **LLM providers**: multi-provider card list (default marked ★), add/edit/remove, enable/disable, vision config; a prominent note that "API keys are environment-variable names only — no key value is saved here".
- **Skills & MCP**: authorized skills/MCP list with kind badges, revoke actions, and scope — global authorizations persist immediately to `runtime.json`, project authorizations to the project's own `project-config.json`.

`[TODO]` skills/MCP authorization enforcement at runtime launch, Claude Code / DSH adapters.

### 5.7 Sessions

- Session list (filter by project): session ID, project, runtime, start time, status.
- Session replay: turns, streamed output, tool calls, **document citations (locator + content hash)**, token usage; read-only replay, never a competing conversation.
- Task progress: current task steps, pending approval requests, cancel entry.

`[TODO]` event trace replay, cancel/approval event stream, task restore from references and snapshots alone.

### 5.8 Changes — the safety core

- ChangeSet list: ID, baseline snapshot hash, target snapshot hash, plan hash, approval state (pending / approved / rejected).
- Change detail: IR diff (component/net additions/removals, semantic annotations), evidence references, verification report.
- **Approval drawer** (slides in from the right): shows bound baseline snapshot + plan hash; when the baseline no longer matches current observation, "approve" is disabled with an explicit note; approve/reject lands in audit.

```text
┌─────────────── approve ChangeSet #12 ────────────────┐
│ baseline: abc123… (matches current observation ✓)    │
│ plan hash: def456…                                   │
│ ┌ IR diff ────────────────────────────────────────┐  │
│ │ + AddComponent U2 (regulator) [evidence→ds_03]  │  │
│ │ + AddNet VOUT  {U2.out → C1.in}                │  │
│ └─────────────────────────────────────────────────┘  │
│ verify: DRC passed · readback inconclusive (amber)   │
│              [ approve ]          [ reject ]         │
└──────────────────────────────────────────────────────┘
```

`[TODO]` diff visualization, approval signature, rollback-handle UI, stale-plan rejection notice.

### 5.9 BOM & Export

- BOM preview (by component/reference/value/evidence reference).
- Export format selection (CSV / Excel / JSON).
- Netlist / SPICE export placeholder.

`[TODO]` BOM generation, netlist / SPICE import-export plugins.

### 5.10 Plugins

Plugin manifest: `id`, `kind` (`agent-runtime` / `eda-backend`), `apiVersion`, `capabilities`, version lock, signature status, permission grants, health, isolation info. Actions: install / update / uninstall, grant and revoke permissions. Discovery validates manifests without executing plugin code.

`[TODO]` signature verification, process isolation, lifecycle supervision, permission revocation.

### 5.11 Usage & Audit

- **Usage**: token usage aggregated by project / provider / runtime, charts, period filter.
- **Audit**: immutable audit log (sessions, approvals, materialization attempts, rollbacks, verification reports), filterable, read-only.

`[TODO]` usage aggregation persistence, audit export.

### 5.12 Settings

Global preferences: theme (light/dark/system), language, data directory, secret-store provider, log level; about (version, branding).

## 6. Key interaction flows

1. **High-risk materialization approval**: Changes → open drawer → show baseline snapshot + plan hash → approve/reject; baseline mismatch disables approve and explains why, lands in audit.
2. **Empty states**: each module's first visit gives a guiding action (e.g. documents empty state leads to "register the first authorized document").
3. **TODO placeholder**: unimplemented entries are clickable into a "coming soon" page listing the target phase and acceptance criteria (referencing `implementation-plan`).
4. **State consistency**: write success / readback success / verification success are shown independently; `inconclusive` is always amber.

## 7. Implementation roadmap

| Milestone | Content | TODO.md phase |
| --- | --- | --- |
| M0 (now) | navigation shell + layout skeleton + design tokens + move existing provider settings into Agents & Tools | Phase 0/1 |
| M1 | read-only Projects / Documents / EDA Services (in-memory data) | Phase 1/2/3 |
| M2 | session replay + read-only usage | Phase 5 |
| M3 | change approval + semantic query + BOM placeholder | Phase 4/7 |
| M4 | plugins + audit + settings | Phase 6 |

## 8. Component & TODO inventory

| Component / capability | Status |
| --- | --- |
| Navigation shell (sidebar + top bar + status bar) | `[TODO]` layout skeleton |
| Design tokens (theme / status colors / spacing / radius) | `[TODO]` establish |
| Agents & tools two-pane page (runtime endpoints / providers / skills & MCP) | implemented |
| Codex App Server start/stop lifecycle + status feedback | implemented |
| Skills/MCP authorization list (global + project scope, persisted immediately) | implemented |
| Claude Code / DSH placeholder adapters | implemented (placeholder detail pages) |
| Read-only Projects / Documents / EDA Services | `[TODO]` |
| Session replay, task progress | `[TODO]` |
| Change approval drawer, IR diff, verification report | `[TODO]` |
| Semantic query, BOM export, netlist/SPICE | `[TODO]` |
| Plugin management, usage, audit, settings | `[TODO]` |
