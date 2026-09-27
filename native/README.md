# Vendored native libraries

Prebuilt native libraries that ship with CircuitFabric. Loaded at runtime; the build never
links against them, and every consumer degrades gracefully when a library is absent.

## `windows-x64/typesafe-mcp/evaluate.exe`

TypeSafe Jev MCP server v0.4.5 (`evaluate`), built from the vendored sources at
`vendor/typesafe-mcp` (<https://github.com/itsmostafa/typesafe-mcp>, License: MIT, see
`LICENSE` next to the binary; the pinned revision is recorded in `VERSION`). It exposes one
`evaluate` tool over stdio MCP: typed judgments (`noul` / `choice` / `score`) with
calibrated probabilities, so agents can branch on a number instead of parsing prose.

Rebuild with `scripts/build-typesafe-mcp.ps1` after updating the submodule. The binary is
registered into the tool catalog automatically at `RuntimeSettings::load_or_default` (see
`circuitfabric-codex-runtime::tools::bundled_mcp_servers`): search order is the
`CIRCUITFABRIC_TYPESAFE_MCP_PATH` environment variable, `typesafe-mcp/evaluate.exe` next to
the executable, then `<ancestor>/native/windows-x64/typesafe-mcp/evaluate.exe` for the first
few ancestors of the executable (covers `target/debug` and `target/debug/deps` during
development). When no binary is found the catalog stays untouched. The server id is
`typesafe-jev`; it requires a `TYPESAFE_API_KEY` value from the secrets vault or the launch
environment, and still needs an explicit authorization grant before agents can call it.

## `windows-x64/pdfium.dll`

PDFium 156.0.8066 (Chromium build 8066), built by <https://github.com/bblanchon/pdfium-binaries>
with `pdf_enable_v8 = false` and `pdf_enable_xfa = false` — the binary contains no JavaScript
engine and no XFA support, so embedded document scripts cannot execute by construction.
License: BSD-3-Clause (see `LICENSE` and `licenses/`); the build arguments are preserved in
`args.gn` and the version in `VERSION`.

Loader search order (see `circuitfabric-document-opener`): the `CIRCUITFABRIC_PDFIUM_PATH`
environment variable, `pdfium.dll` next to the executable, `<ancestor>/native/windows-x64/
pdfium.dll` for the first few ancestors of the executable (covers `target/debug` and
`target/debug/deps` during development), then the system library path. When no library is
found, the PDF opener falls back to in-process text extraction instead of failing.

To update: download `pdfium-win-x64.tgz` from a newer release of the repository above,
extract, and refresh `pdfium.dll`, `LICENSE`, `licenses/`, `args.gn`, and `VERSION`.
