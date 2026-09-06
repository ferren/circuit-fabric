"""Deterministic JSONL MCP server used by native runtime integration tests."""
import json
import sys

for line in sys.stdin:
    request = json.loads(line)
    if 'id' not in request:
        continue
    method = request.get('method')
    if method == 'initialize':
        result = {'protocolVersion': '2024-11-05', 'capabilities': {'tools': {}}, 'serverInfo': {'name': 'cf-fixture', 'version': '1'}}
    elif method == 'tools/list':
        result = {'tools': [{'name': 'echo', 'description': 'Return a test marker', 'inputSchema': {'type': 'object', 'properties': {'text': {'type': 'string'}}, 'required': ['text']}}]}
    elif method == 'tools/call':
        if len(sys.argv) > 1:
            with open(sys.argv[1], 'a', encoding='utf-8') as log:
                log.write(json.dumps(request['params']) + '\n')
        result = {'content': [{'type': 'text', 'text': request['params']['arguments']['text']}], 'isError': False}
    else:
        result = {}
    print(json.dumps({'jsonrpc': '2.0', 'id': request['id'], 'result': result}), flush=True)
