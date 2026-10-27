"""Probe WS server on 49640: accepts /bridge (session) and /eda-operations (executor)
and /probe (probe-extension report). Logs everything to probe_log.jsonl.
"""
import asyncio
import json
import time

import websockets

LOG = r"C:\Users\Ferren\Projects\circuit-fabric\scripts\jlc-acceptance\probe_log.jsonl"


def log(channel, direction, payload):
    entry = {"t": time.strftime("%H:%M:%S"), "channel": channel, "dir": direction}
    try:
        entry["msg"] = json.loads(payload)
    except Exception:
        entry["raw"] = payload[:5000]
    with open(LOG, "a", encoding="utf-8") as fh:
        fh.write(json.dumps(entry, ensure_ascii=False) + "\n")
    print(channel, direction, str(entry.get("msg", entry.get("raw")))[:300], flush=True)


async def handle(ws):
    path = ws.request.path if hasattr(ws, "request") else "/"
    channel = path.strip("/") or "root"
    log(channel, "open", "{}")
    try:
        async for raw in ws:
            log(channel, "recv", raw)
            try:
                msg = json.loads(raw)
            except Exception:
                continue
            if msg.get("type") == "executor_hello":
                await ws.send(json.dumps({
                    "type": "executor_hello_ack", "protocolVersion": 1,
                    "bridge": "CircuitFabric-Probe",
                    "operations": ["inspect", "read_context", "drc", "visual_capture",
                                   "apply", "readback", "rollback"],
                    "unsupportedOperations": []}))
                log(channel, "send", '{"type":"executor_hello_ack"}')
            elif msg.get("type") == "hello":
                await ws.send(json.dumps({
                    "type": "hello_ack", "protocolVersion": 1, "bridge": "CircuitFabric-Probe",
                    "projectId": None, "capabilities": [], "operations": [],
                    "executorConnected": True}))
                log(channel, "send", '{"type":"hello_ack"}')
    except Exception as exc:
        log(channel, "close", json.dumps({"error": str(exc)}))


async def main():
    async with websockets.serve(handle, "127.0.0.1", 49640, max_size=64 * 1024 * 1024):
        print("probe server on 49640", flush=True)
        await asyncio.Future()


asyncio.run(main())
