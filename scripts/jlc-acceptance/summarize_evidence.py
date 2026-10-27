"""Summarize saved real-client evidence without connecting to or modifying EDA.

Usage: python scripts/jlc-acceptance/summarize_evidence.py --history DIR
       --probe-log FILE --output FILE
Inputs remain untouched. This is an evidence comparison, not a fresh acceptance run.
"""

import argparse
import hashlib
import json
from pathlib import Path


def load(path):
    return json.loads(path.read_text(encoding="utf-8-sig"))


def digest(path):
    return "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--history", required=True, type=Path)
    parser.add_argument("--probe-log", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    records = [(path, load(path)) for path in args.history.glob("*.json")]
    records.sort(key=lambda pair: (pair[1].get("capturedAt", ""), pair[0].name))
    if not records:
        parser.error("history contains no JSON observation records")
    observations = []
    for path, record in records:
        snapshot = record.get("snapshot", {})
        payload = record.get("observedPayload", {})
        components = payload.get("components", [])
        observations.append({
            "sourceFile": path.name,
            "sourceHash": digest(path),
            "capturedAt": record.get("capturedAt"),
            "projectId": record.get("projectId"),
            "document": payload.get("document"),
            "snapshotHash": snapshot.get("snapshot_hash"),
            "logicalHash": snapshot.get("logical_hash"),
            "physicalHash": snapshot.get("physical_hash"),
            "externalRefsCount": len(record.get("externalRefs", [])),
            "inconclusive": record.get("inconclusive", []),
            "validation": record.get("validation", []),
            "rawComponentCount": len(components),
            "componentsWithoutDesignator": sum(not c.get("designator") for c in components),
            "snapshotComponentCount": len(snapshot.get("components", [])),
            "snapshotNetCount": len(snapshot.get("nets", [])),
            "wireCount": len(payload.get("wires", [])),
            "pinCount": sum(len(c.get("pins", [])) for c in components),
        })
    comparisons = []
    for (first_path, first), (second_path, second) in zip(records, records[1:]):
        first_snapshot, second_snapshot = first.get("snapshot", {}), second.get("snapshot", {})
        hashes = ["snapshot_hash", "logical_hash", "physical_hash"]
        comparisons.append({
            "first": first_path.name,
            "second": second_path.name,
            "sameProject": first.get("projectId") is not None and first.get("projectId") == second.get("projectId"),
            "sameObservedDocument": bool(first.get("observedPayload", {}).get("document")) and
                first.get("observedPayload", {}).get("document") == second.get("observedPayload", {}).get("document"),
            "hashEquality": {key: bool(first_snapshot.get(key)) and
                first_snapshot.get(key) == second_snapshot.get(key) for key in hashes},
            "externalRefsExactlyEqual": "externalRefs" in first and "externalRefs" in second and
                first["externalRefs"] == second["externalRefs"],
        })
    reports = []
    for line_number, line in enumerate(args.probe_log.read_text(encoding="utf-8-sig").splitlines(), 1):
        if not line.strip():
            continue
        row = json.loads(line)
        message = row.get("msg")
        if row.get("channel") == "probe" and isinstance(message, dict) and "entry" in message:
            reports.append({"sourceLine": line_number, "report": message})
    if not reports:
        parser.error("probe log contains no entry/iframe reports")
    output = {
        "schemaVersion": 1,
        "evidenceKind": "recovered-real-client-records",
        "freshClientAcceptance": False,
        "probeLogHash": digest(args.probe_log),
        "observations": observations,
        "adjacentComparisons": comparisons,
        "probeReports": reports,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(output, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"observationCount": len(records), "probeReportCount": len(reports),
                      "comparisons": comparisons}, ensure_ascii=True, indent=2))


if __name__ == "__main__":
    main()
