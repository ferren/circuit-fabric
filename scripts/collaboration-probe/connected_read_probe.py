"""Read the connected EDA through its existing controlled Bridge.

EDA reads only; Bridge records an audit session and observations in the selected
CircuitFabric project. --project must match the user's selected CF project.
"""
import argparse
import asyncio
import datetime
import json
import os
from pathlib import Path
import time

import websockets


async def run(args):
    args.out.mkdir(parents=True, exist_ok=True)
    records = []
    result = {"schemaVersion": 1, "collectedAt": datetime.datetime.now(datetime.timezone.utc).isoformat(),
              "cfProjectId": args.project, "sessionId": f"probe-584-{time.time_ns()}",
              "scope": "read_context and repeated inspect; no EDA writes", "reads": []}
    async with websockets.connect('ws://127.0.0.1:49630/bridge', open_timeout=5, close_timeout=2,
                                  max_size=32 * 1024 * 1024) as ws:
        async def request(message):
            records.append({"direction": "send", "message": message})
            await ws.send(json.dumps(message))
            deadline = time.monotonic() + 40
            while True:
                frame = json.loads(await asyncio.wait_for(ws.recv(), max(.01, deadline-time.monotonic())))
                records.append({"direction": "recv", "message": frame})
                if frame.get('type') in {'hello_ack', 'project_selected', 'error', 'operation_error'} or frame.get('type', '').endswith('_result'):
                    return frame
        try:
            result['hello'] = await request({"type": "hello", "protocolVersion": 1})
            if result['hello'].get('executorConnected') is not True:
                raise RuntimeError('EDA executor not connected')
            result['selection'] = await request({"type": "select_project", "projectId": args.project})
            if result['selection'].get('type') != 'project_selected':
                raise RuntimeError('CF project selection rejected')
            registry = json.loads((Path(os.environ['APPDATA']) / 'CircuitFabric/projects.json').read_text(encoding='utf-8'))
            root = Path(next(p['canonicalRootPath'] for p in registry['projects'] if p['projectId'] == args.project))
            for operation in ['read_context', 'inspect', 'inspect']:
                reply = await request({"type": operation, "sessionId": result['sessionId'], "document": {"kind": "schematic"}})
                result['reads'].append(reply)
                if reply.get('type') == 'inspect_result' and reply.get('snapshotHash'):
                    filename = reply['snapshotHash'].removeprefix('sha256:') + '.json'
                    record = json.loads((root / 'observations' / filename).read_text(encoding='utf-8'))
                    (args.out / f"inspect-{len(result['reads'])}.json").write_text(json.dumps(record, ensure_ascii=False, indent=2), encoding='utf-8')
        except (OSError, ValueError, KeyError, RuntimeError, TimeoutError) as exc:
            result['error'] = str(exc)
        finally:
            (args.out / 'reads-rpc.jsonl').write_text(''.join(json.dumps(r, ensure_ascii=False)+'\n' for r in records), encoding='utf-8')
            (args.out / 'reads-result.json').write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding='utf-8')
    print(json.dumps(result, ensure_ascii=False))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--project', required=True)
    parser.add_argument('--out', required=True, type=Path)
    asyncio.run(run(parser.parse_args()))
