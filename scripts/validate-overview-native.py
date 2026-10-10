"""Inspect the real Windows desktop with isolated persisted overview fixtures.

Run after `cargo build -p circuitfabric-desktop --features ui-test-support`.
The desktop reads its own DirectX render target; no foreground screen capture is used.
Owns one exact child process at a time, captures actual ring/line/bar pixels,
checks source bytes are unchanged, and closes every window before returning.
No model service, credentials, or user profile is used.
"""
import ctypes
from ctypes import wintypes
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
ARTIFACTS = ROOT / 'target' / 'overview-native'
ARTIFACTS.mkdir(parents=True, exist_ok=True)
NOW = int(time.time())


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2), encoding='utf-8')


def utc(timestamp):
    return datetime.fromtimestamp(timestamp, timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')


def fixture(scenario):
    run_root = ARTIFACTS / f'run-{NOW}' / scenario
    profile = run_root / 'profile'
    settings = profile / 'CircuitFabric' / 'runtime.json'
    write_json(settings, {
        'codex': {'command': 'codex', 'working_directory': str(run_root), 'model': None,
                  'api_key_environment_variable': 'CF_UNUSED_FIXTURE_KEY'},
        'default_provider_id': 'fixture',
        'providers': [{'id': 'fixture', 'name': 'Native fixture', 'kind': 'open_ai_compatible',
                       'base_url': 'http://127.0.0.1:49000/v1', 'model': 'fixture',
                       'api_key_environment_variable': 'CF_UNUSED_FIXTURE_KEY',
                       'native_vision': False, 'enabled': True}],
        'global_preferences': {'language': 'simplified_chinese', 'data_directory': str(profile / 'data')},
        'catalog': {'skills': [], 'mcp_servers': [], 'removed_bundled_servers': ['typesafe-jev']},
        'bridge': {'listen_address': '127.0.0.1:49639'},
    })
    entries = []
    for project_index in range(0 if scenario == 'empty' else 3):
        project_id = f'project-{project_index}'
        root = (run_root / project_id).resolve()
        for folder in ['.circuitfabric', 'sessions', 'documents/datasheets', 'documents/reference-designs', 'logic/snapshots', 'logic/changesets', 'logic/indexes', 'schematics']:
            (root / folder).mkdir(parents=True, exist_ok=True)
        write_json(root / '.circuitfabric/project.json', {'schemaVersion': 1, 'project': {'id': project_id, 'name': ['电源设计', '传感器接口', '空资源项目'][project_index], 'description': 'Isolated native validation fixture'}, 'createdAtUnixSeconds': NOW})
        write_json(root / '.circuitfabric/project-config.json', {'schemaVersion': 1, 'configuration': {'agent_instructions': None, 'enabled_skill_ids': [], 'enabled_mcp_server_ids': []}})
        docs = []
        if scenario != 'zero':
            for index in range([3, 1, 0][project_index]):
                body = f'# Hardware evidence {project_id} / {index}\n'.encode()
                path = f'documents/datasheets/doc-{index}.md'
                (root / path).write_bytes(body)
                docs.append({'id': f'doc-{index}', 'category': 'datasheet', 'originalFileName': f'doc-{index}.md', 'relativePath': path, 'contentHash': 'sha256:' + hashlib.sha256(body).hexdigest(), 'byteSize': len(body), 'documentKind': 'markdown', 'sourceLocator': 'native fixture', 'authorized': index != 2, 'importedAtUnixSeconds': NOW})
        write_json(root / '.circuitfabric/document-index.json', {'schemaVersion': 1, 'documents': docs})
        if scenario != 'zero':
            for index in range([5, 3, 1][project_index]):
                stamp = NOW - 3600 - (index % 5) * 86400
                session_id = f'session-{index}'
                status = ['completed', 'failed', 'running'][index % 3]
                filename = f'{utc(stamp).replace(":", "-")}--{session_id}.md'
                body = '\n'.join(['---', 'schemaVersion: 1', f'sessionId: {session_id}', f'projectId: {project_id}', 'runtimeProfileId: fixture', 'backendId: codex', f'startedAt: {utc(stamp)}', f'completedAt: {utc(stamp + 10) if status != "running" else "null"}', f'status: {status}', 'usage: { inputTokens: 0, outputTokens: 0 }', 'citations: []', '---', '', '## 会话摘要', '持久化验收记录；failed 记录包含取消任务。'])
                (root / 'sessions' / filename).write_text(body, encoding='utf-8')
            if project_index < 2:
                for name, parent in [('old', None), ('tip', 'old')]:
                    if name == 'old' and project_index == 1:
                        continue
                    snapshot = {'schema_version': 1, 'snapshot_hash': name, 'logical_hash': name, 'physical_hash': None, 'parent_snapshot_hash': parent, 'authority': 'observed', 'components': [], 'nets': [], 'application_semantics': {}, 'evidence': [], 'constraints': []}
                    for index, status in enumerate(['passed', 'failed', 'inconclusive', 'not_run']):
                        snapshot['constraints'].append({'constraint_id': f'rule-{index}', 'layer': 'logic', 'subject_refs': [], 'status': status, 'severity': 'warning', 'evidence_refs': [], 'snapshot_hash': name, 'explanation': f'{status} persisted evidence', 'recommendation': None})
                    write_json(root / f'logic/snapshots/{name}.json', snapshot)
                decisions = [None, 'approved', 'rejected', 'cancelled', 'awaiting_approval'] if project_index == 0 else [None]
                for index, decision in enumerate(decisions):
                    change_id = f'change-{index}'
                    write_json(root / f'logic/changesets/{change_id}.json', {'schemaVersion': 1, 'id': change_id, 'baseSnapshotHash': 'old', 'targetSnapshotHash': 'tip', 'planHash': 'plan', 'irDiff': ['fixture change'], 'evidence': [], 'execution': {}, 'observedSnapshotHash': None, 'rollbackHandle': None, 'audit': [] if decision is None else [{'timestampUnixSeconds': NOW, 'actor': 'fixture reviewer', 'decision': decision, 'reason': 'persisted review', 'observedSnapshotHash': None, 'rollbackHandle': None}]})
        if scenario == 'partial' and project_index == 1:
            (root / 'logic/changesets/broken.json').write_text('corrupt', encoding='utf-8')
        entries.append({'projectId': project_id, 'canonicalRootPath': str(root), 'displayName': ['电源设计', '传感器接口', '空资源项目'][project_index], 'lastOpenedUnixSeconds': NOW})
    write_json(settings.with_name('projects.json'), {'schemaVersion': 1, 'projects': entries})
    return profile, [Path(entry['canonicalRootPath']) for entry in entries]


user32 = ctypes.windll.user32
user32.SetProcessDPIAware()
user32.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
user32.GetWindowRect.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.RECT)]
user32.MoveWindow.argtypes = [wintypes.HWND, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int, wintypes.BOOL]
user32.PostMessageW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]
user32.ShowWindow.argtypes = [wintypes.HWND, ctypes.c_int]
user32.SetForegroundWindow.argtypes = [wintypes.HWND]
user32.GetForegroundWindow.restype = wintypes.HWND
user32.BringWindowToTop.argtypes = [wintypes.HWND]
user32.GetDpiForWindow.argtypes = [wintypes.HWND]
callback_type = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)


def find_window(pid):
    found = []
    @callback_type
    def callback(hwnd, _):
        owner = wintypes.DWORD()
        user32.GetWindowThreadProcessId(hwnd, ctypes.byref(owner))
        if owner.value == pid and user32.IsWindowVisible(hwnd):
            title = ctypes.create_unicode_buffer(256)
            user32.GetWindowTextW(hwnd, title, 256)
            if title.value.startswith('CircuitFabric'):
                found.append(hwnd)
        return True
    user32.EnumWindows(callback, 0)
    return found[0] if found else None


def rectangle(hwnd):
    rect = wintypes.RECT()
    user32.GetWindowRect(hwnd, ctypes.byref(rect))
    return rect.left, rect.top, rect.right, rect.bottom


def fingerprint(roots):
    return {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for root in roots for path in root.rglob('*') if path.is_file()}


def painted_chart_pixels(image, scale):
    # Fixed validation viewport; exclude titles, legends, and numerical labels.
    regions = {'ring': (285, 415, 450, 575), 'line': (885, 448, 1370, 600),
               'bars': (285, 820, 1320, 1030)}
    colors = {'passed': (22, 163, 74), 'failed': (185, 28, 28),
              'inconclusive': (180, 83, 9), 'not_run': (71, 85, 105),
              'documents': (139, 92, 246), 'sessions': (34, 211, 238),
              'snapshots': (59, 130, 246)}
    result = {}
    for name, bounds in regions.items():
        crop = image.crop(tuple(round(value * scale) for value in bounds))
        pixels = list(crop.get_flattened_data())
        result[name] = {label: sum(all(abs(pixel[i] - rgb[i]) < 12 for i in range(3))
                                   for pixel in pixels) for label, rgb in colors.items()}
        if name == 'line':
            xs = [index % crop.width for index, pixel in enumerate(pixels)
                  if all(abs(pixel[i] - colors['sessions'][i]) < 12 for i in range(3))]
            result[name]['horizontal_span_logical_pixels'] = (max(xs) - min(xs)) / scale if xs else 0
    assert all(result['ring'][label] > 100 for label in ['passed', 'failed', 'inconclusive', 'not_run']), result
    assert all(result['bars'][label] > 100 for label in ['documents', 'sessions', 'snapshots']), result
    assert result['line']['sessions'] > 100 and result['line']['horizontal_span_logical_pixels'] > 200, result
    return result


def inspect(scenario, size='1400,1400', scroll=False):
    from PIL import Image
    profile, roots = fixture(scenario)
    before = fingerprint(roots)
    name = scenario + ('-narrow-scrolled' if scroll else '-narrow' if size != '1400,1400' else '')
    path = ARTIFACTS / (name + '.png')
    env = dict(os.environ, APPDATA=str(profile), CF_OVERVIEW_CAPTURE=str(path), CF_OVERVIEW_CAPTURE_SIZE=size)
    with (ARTIFACTS / (name + '.log')).open('w', encoding='utf-8') as log:
        launched_at = time.time()
        child = subprocess.Popen([str(ROOT / 'target/debug/circuitfabric-desktop.exe')], cwd=ROOT, env=env, stdout=log, stderr=log)
        try:
            if scroll:
                hwnd = None
                for _ in range(20):
                    hwnd = find_window(child.pid)
                    if hwnd: break
                    time.sleep(0.1)
                if not hwnd: raise RuntimeError('owned window missing for scroll check')
                time.sleep(0.6)
                scale = user32.GetDpiForWindow(hwnd) / 96
                rect = rectangle(hwnd)
                user32.PostMessageW(hwnd, 0x020A, ((-8 * 120) & 0xffff) << 16, ((rect[1] + round(550 * scale)) << 16) | (rect[0] + round(650 * scale)))
            child.wait(timeout=20)
            if child.returncode != 0: raise RuntimeError('native desktop capture failed; inspect ' + name + '.log')
        finally:
            if child.poll() is None:
                hwnd = find_window(child.pid)
                if hwnd: user32.PostMessageW(hwnd, 0x0010, 0, 0)
                try: child.wait(timeout=5)
                except subprocess.TimeoutExpired: child.terminate(); child.wait(timeout=5)
    assert path.stat().st_mtime >= launched_at and path.with_suffix('.json').stat().st_mtime >= launched_at, 'capture must come from this exact run, not old artifacts'
    metadata = json.loads(path.with_suffix('.json').read_text(encoding='utf-8'))
    image = Image.open(path).convert('RGB')
    assert before == fingerprint(roots), 'overview browsing must not modify domain/audit files'
    if scenario == 'populated':
        assert [metadata[key] for key in ['projects', 'documents', 'pending', 'sessions']] == [3, 3, 3, 9], metadata
        assert metadata['facts'] == [2, 2, 2, 2]
        assert [row[1] for row in metadata['resources']] == [[2, 5, 2], [1, 3, 1], [0, 1, 0]]
    if scenario == 'partial':
        assert metadata['loaded_projects'] == 2 and metadata['resources'][1][1] is None
        assert [metadata[key] for key in ['documents', 'pending', 'sessions']] == [2, 2, 6]
        assert metadata['facts'] == [1, 1, 1, 1]
    if scenario in ['empty', 'zero']:
        assert [metadata[key] for key in ['documents', 'pending', 'sessions']] == [0, 0, 0]
        assert metadata['facts'] == [0, 0, 0, 0]
    if size != '1400,1400':
        assert 849 <= metadata['viewport'][0] <= 851 and 699 <= metadata['viewport'][1] <= 701
    result = {'scenario': name, 'source_bytes_unchanged': True, 'child_closed': True, 'metadata': metadata}
    if name == 'populated':
        result['paint_coverage'] = painted_chart_pixels(image, metadata['scale'])
    print(json.dumps(result), flush=True)
    return result


if __name__ == '__main__':
    results = []
    for scenario in ['empty', 'zero', 'populated', 'partial']:
        results.append(inspect(scenario))
    results.append(inspect('populated', '850,700'))
    results.append(inspect('populated', '850,700', scroll=True))
    write_json(ARTIFACTS / 'native-validation-results.json', results)
