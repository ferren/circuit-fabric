"""Regression checks for discovery evidence; these are mocks, not EDA acceptance."""
import asyncio
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from bridge_probe import discover


class Socket:
    def __init__(self):
        self.replies = iter([
            {"type": "hello_ack", "executorConnected": False},
            {"type": "status_ack", "executorConnected": False},
            {"type": "projects", "projects": [{"id": "cf-registry-project"}]},
        ])
        self.requests = []

    async def __aenter__(self):
        return self

    async def __aexit__(self, *args):
        pass

    async def send(self, raw):
        self.requests.append(json.loads(raw))

    async def recv(self):
        return json.dumps(next(self.replies))


class DiscoveryTests(unittest.TestCase):
    def test_registry_and_hello_never_claim_native_lifecycle(self):
        socket = Socket()
        with tempfile.TemporaryDirectory() as directory, patch('bridge_probe.websockets.connect', return_value=socket):
            asyncio.run(discover(Path(directory), 49630))
            result = json.loads((Path(directory) / 'bridge-result.json').read_text(encoding='utf-8'))
            self.assertEqual(result['hostLifecycle'], 'unverified')
            self.assertFalse(result['hello'][0]['executorConnected'])
            self.assertEqual([x['type'] for x in socket.requests], ['hello', 'status', 'list_projects'])
            self.assertNotIn('nativeProjectTree', result)
            self.assertEqual(len((Path(directory) / 'bridge-rpc.jsonl').read_text(encoding='utf-8').splitlines()), 6)

    def test_unreachable_bridge_persists_failure(self):
        with tempfile.TemporaryDirectory() as directory, patch('bridge_probe.websockets.connect', side_effect=OSError('connection refused')):
            asyncio.run(discover(Path(directory), 49630))
            result = json.loads((Path(directory) / 'bridge-result.json').read_text(encoding='utf-8'))
            self.assertEqual(result['error'], 'connection refused')
            self.assertEqual(result['hostLifecycle'], 'unverified')


if __name__ == '__main__':
    unittest.main()
