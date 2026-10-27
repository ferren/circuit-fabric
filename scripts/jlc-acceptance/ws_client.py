"""CircuitFabric JLC bridge session-channel client for manual acceptance.

Usage:
  python scripts/jlc-acceptance/ws_client.py <command> [args]

Commands:
  hello                 -> hello + status + list_projects
  select <projectId>    -> select project
  op <operation>        -> send operation (inspect/drc/visual_capture/read_context), print all frames until terminal
  preview <jsonFile>    -> send preview message from JSON file
  approve <planHash> <baselineHash> <decision>
  apply <planHash>
  readback <planHash>
  rollback <planHash>
  cancel-after <operation> <ms>  -> send op, then cancel after <ms> milliseconds
  raw <jsonFile>        -> send raw JSON, print frames until idle timeout
"""
import asyncio
import json
import sys

import websockets

URL = "ws://127.0.0.1:49630/bridge"
SESSION = "acceptance-2026-10-23"


async def recv_until_terminal(ws, timeout=40):
    frames = []
    try:
        while True:
            raw = await asyncio.wait_for(ws.recv(), timeout)
            msg = json.loads(raw)
            frames.append(msg)
            t = msg.get("type", "")
            if t.endswith("_result") or t in ("error", "operation_error", "pong", "status_ack",
                                              "projects", "project_selected", "hello_ack",
                                              "sessions", "session_loaded"):
                # hello_ack/projects/project_selected are terminal for those requests
                return frames
    except asyncio.TimeoutError:
        pass
    return frames


async def main():
    cmd = sys.argv[1]
    async with websockets.connect(URL, max_size=32 * 1024 * 1024) as ws:
        async def send(obj):
            await ws.send(json.dumps(obj))

        if cmd == "hello":
            await send({"type": "hello", "protocolVersion": 1})
            print(json.dumps(await recv_until_terminal(ws), ensure_ascii=False, indent=1))
            await send({"type": "status"})
            print(json.dumps(await recv_until_terminal(ws), ensure_ascii=False, indent=1))
            await send({"type": "list_projects"})
            print(json.dumps(await recv_until_terminal(ws), ensure_ascii=False, indent=1))
            return

        # all remaining commands need hello + select first
        await send({"type": "hello", "protocolVersion": 1})
        await recv_until_terminal(ws)

        if cmd == "select":
            await send({"type": "select_project", "projectId": sys.argv[2]})
            print(json.dumps(await recv_until_terminal(ws), ensure_ascii=False, indent=1))
            return

        await send({"type": "select_project", "projectId": "MojianC3"})
        await recv_until_terminal(ws)

        if cmd == "op":
            operation = sys.argv[2]
            await send({"type": operation, "sessionId": SESSION, "document": {"kind": "schematic"}})
            frames = await recv_until_terminal(ws)
            print(json.dumps(frames, ensure_ascii=False, indent=1))
        elif cmd == "cancel-after":
            operation = sys.argv[2]
            delay = float(sys.argv[3]) / 1000.0
            await send({"type": operation, "sessionId": SESSION, "document": {"kind": "schematic"}})
            frames = []
            # wait for operation_started to learn requestId
            request_id = None
            try:
                while True:
                    msg = json.loads(await asyncio.wait_for(ws.recv(), 10))
                    frames.append(msg)
                    if msg.get("type") == "operation_started":
                        request_id = msg.get("requestId")
                        break
                    if msg.get("type", "").endswith("_result") or msg.get("type") == "operation_error":
                        break
            except asyncio.TimeoutError:
                pass
            if request_id:
                await asyncio.sleep(delay)
                await send({"type": "cancel_operation", "requestId": request_id})
                frames += await recv_until_terminal(ws)
            print(json.dumps(frames, ensure_ascii=False, indent=1))
        elif cmd == "preview":
            with open(sys.argv[2], encoding="utf-8") as fh:
                message = json.load(fh)
            message.setdefault("sessionId", SESSION)
            await send(message)
            print(json.dumps(await recv_until_terminal(ws), ensure_ascii=False, indent=1))
        elif cmd in ("approve", "apply", "readback", "rollback"):
            if cmd == "approve":
                await send({"type": "approve_plan", "sessionId": SESSION,
                            "planHash": sys.argv[2], "baselineSnapshotHash": sys.argv[3],
                            "decision": sys.argv[4]})
            else:
                await send({"type": cmd, "sessionId": SESSION, "planHash": sys.argv[2]})
            print(json.dumps(await recv_until_terminal(ws, timeout=90), ensure_ascii=False, indent=1))
        elif cmd == "raw":
            with open(sys.argv[2], encoding="utf-8") as fh:
                await send(json.load(fh))
            print(json.dumps(await recv_until_terminal(ws), ensure_ascii=False, indent=1))


asyncio.run(main())
