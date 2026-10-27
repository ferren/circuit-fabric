import asyncio
import json
import sys

import websockets

URL = "ws://127.0.0.1:49630/bridge"
SESSION = "acceptance-2026-10-23"


async def main():
    async with websockets.connect(URL, max_size=32 * 1024 * 1024) as ws:
        await ws.send(json.dumps({"type": "hello", "protocolVersion": 1}))
        hello = json.loads(await ws.recv())
        print("hello_ack executorConnected:", hello.get("executorConnected"))
        await ws.send(json.dumps({"type": "select_project", "projectId": "MojianC3"}))
        sel = json.loads(await ws.recv())
        print("project_selected:", sel.get("projectId"), "ops:", sel.get("operations"))

        for op in ["drc", "visual_capture"]:
            await ws.send(json.dumps({"type": op, "sessionId": SESSION,
                                      "document": {"kind": "schematic"}}))
            while True:
                msg = json.loads(await asyncio.wait_for(ws.recv(), 60))
                t = msg.get("type")
                if t == "operation_progress":
                    print(f"  [{op}] progress:", msg.get("phase"))
                    continue
                if t in ("drc_result", "visual_capture_result", "operation_error", "error"):
                    print(f"[{op}] result:", json.dumps(msg, ensure_ascii=False)[:800])
                    break


asyncio.run(main())
