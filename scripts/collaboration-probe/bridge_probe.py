"""Bounded real Bridge discovery; may start a task-owned Bridge, always stops it.

No model calls or EDA writes. Output is evidence of connection, not C14 support.
"""
import argparse
import asyncio
import datetime
import json
from pathlib import Path
import subprocess
import time

import websockets


async def discover(out, port):
    trace = []
    result = {"schemaVersion": 1, "collectedAt": datetime.datetime.now(datetime.timezone.utc).isoformat(),
              "scope": "real bridge hello/status/list_projects only", "hostLifecycle": "unverified"}
    try:
        async with websockets.connect(f"ws://127.0.0.1:{port}/bridge", open_timeout=3, close_timeout=1) as ws:
            for request in [{"type": "hello", "protocolVersion": 1}, {"type": "status"}, {"type": "list_projects"}]:
                trace.append({"direction": "send", "message": request})
                await ws.send(json.dumps(request))
                frames = []
                for _ in range(20):
                    reply = json.loads(await asyncio.wait_for(ws.recv(), 5))
                    frames.append(reply)
                    trace.append({"direction": "recv", "message": reply})
                    if reply.get("type") in {"hello_ack", "status_ack", "projects", "error"}:
                        break
                result[request["type"]] = frames
    except (OSError, TimeoutError, websockets.WebSocketException) as exc:
        result["error"] = str(exc)
    (out / "bridge-rpc.jsonl").write_text("".join(json.dumps(x, ensure_ascii=False) + "\n" for x in trace), encoding="utf-8")
    (out / "bridge-result.json").write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding="utf-8")
    print(json.dumps(result, ensure_ascii=False))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--port", type=int, default=49630)
    parser.add_argument("--start-bridge", type=Path)
    parser.add_argument("--settings", type=Path)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    if not args.start_bridge:
        asyncio.run(discover(args.out, args.port))
        return
    if not args.settings:
        parser.error("--start-bridge requires --settings; never guess user configuration")
    with (args.out / "bridge-stdout.txt").open("w", encoding="utf-8") as stdout, (args.out / "bridge-stderr.txt").open("w", encoding="utf-8") as stderr:
        child = subprocess.Popen([str(args.start_bridge.resolve()), str(args.settings.resolve())], stdout=stdout, stderr=stderr)
        try:
            time.sleep(8)  # One bounded startup window for the real extension to reconnect.
            if child.poll() is not None:
                raise RuntimeError("Probe Bridge did not start; inspect stderr")
            asyncio.run(discover(args.out, args.port))
        finally:
            if child.poll() is None:
                child.terminate()
            child.wait(timeout=5)


if __name__ == "__main__":
    main()
