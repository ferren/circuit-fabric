"""Exercise installed runtime binaries against a local deterministic model service.

No remote model or real credential is used. Run after building runtime_probe.
"""
import http.server
import json
import os
from pathlib import Path
import subprocess
import tempfile
import threading
import sys
import time
import struct
import zlib

requests = []
tool_names = []
skill_seen = []
tool_results = []
image_seen = []


class Model(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers.get('Content-Length', 0))) or b'{}')
        requests.append((self.path, body.get('model')))
        image_seen.append('input_image' in json.dumps(body))
        if 'CF_WAIT_FOR_CANCEL' in json.dumps(body):
            time.sleep(4)
            try:
                self.send_response(503)
                self.end_headers()
            except ConnectionError:
                pass  # Cancellation deliberately closes the client connection.
            return
        tool_names.append(body.get('tools'))
        skill_seen.append('CF_SKILL_A' in json.dumps(body) and 'CF_SKILL_B' in json.dumps(body))
        tool_results.extend(x for x in body.get('input', []) if isinstance(x, dict) and x.get('type') == 'function_call_output')
        if 'count_tokens' in self.path:
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.end_headers()
            self.wfile.write(b'{"input_tokens":10}')
            return
        self.send_response(200)
        self.send_header('Content-Type', 'text/event-stream')
        self.end_headers()

        def event(kind, value):
            self.wfile.write(f'event: {kind}\ndata: {json.dumps(value)}\n\n'.encode())

        if '/responses' in self.path:
            if not any(x.get('type') == 'function_call_output' for x in body.get('input', []) if isinstance(x, dict)):
                calls = [{'id': f'fc_{name}', 'type': 'function_call', 'call_id': f'call_{name}', 'name': 'echo', 'namespace': f'mcp__{name}', 'arguments': json.dumps({'text': f'CF_TOOL_{name}'})} for name in ['first', 'second']]
                for i, call in enumerate(calls):
                    event('response.output_item.added', {'type': 'response.output_item.added', 'output_index': i, 'item': {**call, 'arguments': ''}})
                    event('response.function_call_arguments.delta', {'type': 'response.function_call_arguments.delta', 'output_index': i, 'item_id': call['id'], 'delta': call['arguments']})
                    event('response.output_item.done', {'type': 'response.output_item.done', 'output_index': i, 'item': call})
                event('response.completed', {'type': 'response.completed', 'response': {'id': 'resp_tools', 'status': 'completed', 'output': calls, 'usage': {'input_tokens': 10, 'output_tokens': 2, 'total_tokens': 12}}})
                self.wfile.flush()
                return
            msg = {'id': 'msg_test', 'type': 'message', 'role': 'assistant', 'status': 'completed', 'content': [{'type': 'output_text', 'text': 'CF_OK', 'annotations': []}]}
            response = {'id': 'resp_test', 'object': 'response', 'status': 'completed', 'output': [msg], 'usage': {'input_tokens': 10, 'output_tokens': 2, 'total_tokens': 12}}
            event('response.created', {'type': 'response.created', 'response': {**response, 'status': 'in_progress', 'output': []}})
            event('response.output_item.added', {'type': 'response.output_item.added', 'output_index': 0, 'item': {**msg, 'status': 'in_progress', 'content': []}})
            event('response.content_part.added', {'type': 'response.content_part.added', 'item_id': 'msg_test', 'output_index': 0, 'content_index': 0, 'part': {'type': 'output_text', 'text': '', 'annotations': []}})
            event('response.output_text.delta', {'type': 'response.output_text.delta', 'item_id': 'msg_test', 'output_index': 0, 'content_index': 0, 'delta': 'CF_OK'})
            event('response.output_text.done', {'type': 'response.output_text.done', 'item_id': 'msg_test', 'output_index': 0, 'content_index': 0, 'text': 'CF_OK'})
            event('response.output_item.done', {'type': 'response.output_item.done', 'output_index': 0, 'item': msg})
            event('response.completed', {'type': 'response.completed', 'response': response})
        elif '/messages' in self.path:
            event('message_start', {'type': 'message_start', 'message': {'id': 'msg_test', 'type': 'message', 'role': 'assistant', 'model': body.get('model'), 'content': [], 'stop_reason': None, 'usage': {'input_tokens': 10, 'output_tokens': 0}}})
            if not any('tool_result' in json.dumps(x) for x in body.get('messages', [])):
                for i, name in enumerate(['first', 'second']):
                    event('content_block_start', {'type': 'content_block_start', 'index': i, 'content_block': {'type': 'tool_use', 'id': f'tool_{name}', 'name': f'mcp__{name}__echo', 'input': {}}})
                    event('content_block_delta', {'type': 'content_block_delta', 'index': i, 'delta': {'type': 'input_json_delta', 'partial_json': json.dumps({'text': f'CF_TOOL_{name}'})}})
                    event('content_block_stop', {'type': 'content_block_stop', 'index': i})
                event('message_delta', {'type': 'message_delta', 'delta': {'stop_reason': 'tool_use', 'stop_sequence': None}, 'usage': {'output_tokens': 2}})
                event('message_stop', {'type': 'message_stop'})
                self.wfile.flush()
                return
            event('content_block_start', {'type': 'content_block_start', 'index': 0, 'content_block': {'type': 'text', 'text': ''}})
            event('content_block_delta', {'type': 'content_block_delta', 'index': 0, 'delta': {'type': 'text_delta', 'text': 'CF_OK'}})
            event('content_block_stop', {'type': 'content_block_stop', 'index': 0})
            event('message_delta', {'type': 'message_delta', 'delta': {'stop_reason': 'end_turn', 'stop_sequence': None}, 'usage': {'output_tokens': 2}})
            event('message_stop', {'type': 'message_stop'})
        else:
            if not any(x.get('role') == 'tool' for x in body.get('messages', [])):
                calls = [{'index': i, 'id': f'call_{name}', 'type': 'function', 'function': {'name': f'mcp__{name}__echo', 'arguments': json.dumps({'text': f'CF_TOOL_{name}'})}} for i, name in enumerate(['first', 'second'])]
                for delta, reason in [({'role': 'assistant', 'tool_calls': calls}, None), ({}, 'tool_calls')]:
                    value = {'id': 'chat_tools', 'object': 'chat.completion.chunk', 'created': 1, 'model': body.get('model'), 'choices': [{'index': 0, 'delta': delta, 'finish_reason': reason}]}
                    self.wfile.write(f'data: {json.dumps(value)}\n\n'.encode())
                self.wfile.write(b'data: [DONE]\n\n')
                self.wfile.flush()
                return
            for delta, reason in [({'role': 'assistant', 'content': 'CF_OK'}, None), ({}, 'stop')]:
                value = {'id': 'chat_test', 'object': 'chat.completion.chunk', 'created': 1, 'model': body.get('model'), 'choices': [{'index': 0, 'delta': delta, 'finish_reason': reason}]}
                self.wfile.write(f'data: {json.dumps(value)}\n\n'.encode())
            self.wfile.write(b'data: [DONE]\n\n')
        self.wfile.flush()


if __name__ == '__main__':
    repo = Path(__file__).resolve().parents[3]
    binary = repo / 'target/debug/examples/runtime_probe.exe'
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Model)
    worker = threading.Thread(target=server.serve_forever)
    worker.start()
    try:
        with tempfile.TemporaryDirectory(prefix='runtime-smoke-', dir=repo / 'target') as directory:
            provider = {'id': 'local', 'name': 'Local fixture', 'base_url': f'http://127.0.0.1:{server.server_port}/v1', 'model': 'claude-sonnet-4-6', 'api_key_environment_variable': 'CF_SMOKE_API_KEY', 'enabled': True, 'supports_vision': False}
            settings = {'codex': {'command': 'codex', 'working_directory': directory, 'model': None, 'api_key_environment_variable': 'CF_SMOKE_API_KEY'}, 'default_provider_id': 'local', 'providers': [provider], 'bridge': {'listen_address': '127.0.0.1:49630'}}
            skills = []
            for name in ['a', 'b']:
                skill = Path(directory) / name / 'SKILL.md'
                skill.parent.mkdir()
                skill.write_text(f'# Skill {name}\nCF_SKILL_{name.upper()}', encoding='utf-8')
                skills.append({'id': name, 'path': str(skill), 'enabled': True})
            settings['catalog'] = {'skills': skills, 'mcp_servers': [{'id': name, 'command': sys.executable, 'args': [str(Path(__file__).with_name('mcp_fixture.py').resolve()), str(Path(directory) / f'{name}.calls')], 'environment_variables': [], 'enabled': True} for name in ['first', 'second']]}
            settings['tools'] = {'authorized_skill_ids': ['a', 'b'], 'authorized_mcp_server_ids': ['first', 'second']}
            path = Path(directory) / 'runtime.json'
            path.write_text(json.dumps(settings), encoding='utf-8')
            environment = {**os.environ, 'CF_SMOKE_API_KEY': 'fixture-only', 'TEMP': directory, 'TMP': directory}
            failed = False
            for kind in sys.argv[1:] or ['mcp', 'codex', 'claude', 'dsh']:
                start = len(skill_seen)
                for name in ['first', 'second']:
                    (Path(directory) / f'{name}.calls').write_text('', encoding='utf-8')
                result = subprocess.run([str(binary), str(path), kind, 'Return CF_OK only.'], env=environment, capture_output=True, text=True, encoding='utf-8', errors='replace', timeout=220)
                ok = result.returncode == 0 and 'CF_OK' in result.stdout
                print(f'{kind}: {"PASS" if ok else "FAIL"}', flush=True)
                if not ok:
                    print(result.stdout[-1000:], result.stderr[-10000:], flush=True)
                    failed = True
                if kind != 'mcp' and ok:
                    visible = json.dumps(tool_names[start:])
                    if 'first' not in visible: print('Tool definitions:', visible[:4000])
                    assert 'first' in visible and 'second' in visible, f'{kind}: MCP tools not exposed'
                    assert any(skill_seen[start:]), f'{kind}: skill instructions missing'
                    for name in ['first', 'second']:
                        assert f'CF_TOOL_{name}' in (Path(directory) / f'{name}.calls').read_text(), f'{kind}: {name} was not called: {tool_results}'
            print('Local model requests:', requests)
            if tool_names:
                print('First tool schema names:', [(x.get('type'), x.get('name'), [t.get('name') for t in x.get('tools', [])]) for x in (tool_names[0] or [])])
            if failed:
                raise SystemExit(1)
            # Explicit binding overrides the default, including the actual service route.
            alternate = {**provider, 'id': 'alternate', 'base_url': f'http://127.0.0.1:{server.server_port}/alternate/v1', 'model': 'fixture-alternate'}
            settings['providers'].append(alternate)
            settings['adapters'] = {'claude_command': 'claude', 'dsh_command': 'dsh', 'codex_provider_id': 'alternate'}
            path.write_text(json.dumps(settings), encoding='utf-8')
            result = subprocess.run([str(binary), str(path), 'codex', 'CF_OK'], env=environment, capture_output=True, text=True, timeout=220)
            assert result.returncode == 0, result.stderr
            assert ('/alternate/v1/responses', 'fixture-alternate') in requests
            print('provider binding and service route: PASS', flush=True)
            # Vision overrides the language model and includes image bytes in the request.
            alternate.update(supports_vision=True, vision_base_url=f'http://127.0.0.1:{server.server_port}/vision/v1', vision_model='fixture-vision', vision_api_key_environment_variable='CF_SMOKE_API_KEY')
            picture = Path(directory) / 'pixel.png'
            def png_chunk(kind, data):
                return struct.pack('!I', len(data)) + kind + data + struct.pack('!I', zlib.crc32(kind + data))
            picture.write_bytes(b'\x89PNG\r\n\x1a\n' + png_chunk(b'IHDR', struct.pack('!2I5B', 32, 32, 8, 2, 0, 0, 0)) + png_chunk(b'IDAT', zlib.compress((b'\0' + b'\xff\0\0' * 32) * 32)) + png_chunk(b'IEND', b''))
            path.write_text(json.dumps(settings), encoding='utf-8')
            result = subprocess.run([str(binary), str(path), 'codex', 'CF_OK', str(picture)], env=environment, capture_output=True, text=True, timeout=220)
            assert result.returncode == 0, result.stderr
            assert ('/vision/v1/responses', 'fixture-vision') in requests and any(image_seen), (requests, image_seen, result.stdout, result.stderr)
            print('vision service and image input: PASS', flush=True)
            for kind in ['codex', 'claude', 'dsh']:
                result = subprocess.run([str(binary), str(path), kind, 'CF_WAIT_FOR_CANCEL', 'cancel'], env=environment, capture_output=True, text=True, timeout=30)
                assert result.returncode == 0 and 'CF_CANCEL_OK' in result.stdout, (kind, result.stderr)
                assert not list(Path(directory).glob('circuitfabric-run-*')), 'runtime temporary directories leaked'
                print(f'{kind} cancellation and cleanup: PASS', flush=True)
    finally:
        server.shutdown()
        server.server_close()
        worker.join()
