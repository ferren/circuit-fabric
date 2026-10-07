"""Check desktop dependency direction and shared UI policies (stdlib only)."""
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "apps/circuitfabric-desktop/src"
errors = []

for path in (SRC / "application").rglob("*.rs"):
    source = path.read_text(encoding="utf-8")
    for line_number, line in enumerate(source.splitlines(), 1):
        if line.lstrip().startswith("//"):
            continue
        if re.search(r"\b(?:gpui|gpui_component|gpui_base|crate::ui)\s*::", line):
            errors.append(f"{path.relative_to(ROOT)}:{line_number}: application must not depend on UI")

for path in (SRC / "ui").rglob("*.rs"):
    source = path.read_text(encoding="utf-8")
    if path.name != "widgets.rs":
        for line_number, line in enumerate(source.splitlines(), 1):
            if line.lstrip().startswith("//"):
                continue
            if re.search(r"\bButton\s*::\s*new\s*\(", line):
                errors.append(f"{path.relative_to(ROOT)}:{line_number}: use widgets::action_button for intrinsic button width")
    # PDF navigation deliberately owns a two-axis ScrollHandle for page anchors,
    # zoom and pan; ordinary UI regions must use the shared layout policy.
    if path.name in {"layout.rs", "preview.rs"}:
        continue
    for line_number, line in enumerate(source.splitlines(), 1):
        if line.lstrip().startswith("//"):
            continue
        if re.search(r"\.(?:overflow_[xy]_scroll(?:bar)?|overflow_scroll(?:bar)?|vertical_scrollbar|horizontal_scrollbar)\s*\(", line):
            errors.append(f"{path.relative_to(ROOT)}:{line_number}: use layout::ScrollRegionExt")

if errors:
    print("\n".join(errors), file=sys.stderr)
    sys.exit(1)
print("Desktop boundaries OK: application is UI-independent; scrolling and buttons use shared policies.")
