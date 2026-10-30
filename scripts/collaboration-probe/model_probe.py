"""Bounded native Codex CLI inference/resume probe; separate from EDA integration.

Uses the runtime's existing authentication/configuration without copying secrets.
Only a deterministic test image and a random test token are sent to the model.
"""
import argparse
import datetime
import json
import os
from pathlib import Path
import struct
import subprocess
import uuid
import zlib
from runtime_probe import command


def run(argv, cwd, timeout=55):
    proc = subprocess.Popen(argv, cwd=cwd, stdin=subprocess.DEVNULL,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    timed_out = False
    try:
        stdout, stderr = proc.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        timed_out = True
        if os.name == 'nt' and proc.poll() is None:
            subprocess.run(['taskkill', '/PID', str(proc.pid), '/T', '/F'],
                           capture_output=True, timeout=10)
        else:
            proc.kill()
        stdout, stderr = proc.communicate(timeout=5)
    return dict(exitCode=proc.returncode, timedOut=timed_out,
                stdout=stdout.decode('utf-8', errors='replace'),
                stderr=stderr.decode('utf-8', errors='replace'))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', required=True, type=Path)
    args = parser.parse_args()
    folder = args.out.resolve()
    folder.mkdir(parents=True, exist_ok=True)
    image = folder / 'fixture.png'
    def chunk(kind, data):
        return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data))
    pixels = b''.join(b'\x00' + b'\xff\x00\x00' * 40 + b'\x00\xff\x00' * 40 + b'\x00\x00\xff' * 40 for _ in range(40))
    image.write_bytes(b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', 120, 40, 8, 2, 0, 0, 0))
                      + chunk(b'IDAT', zlib.compress(pixels)) + chunk(b'IEND', b''))
    binary = command('codex')
    result = dict(schemaVersion=1, collectedAt=datetime.datetime.now(datetime.timezone.utc).isoformat(),
                  scope='native CLI only; no EDA tool task; no interactive question/answer test',
                  version=run([binary, '--version'], folder), imageInput='unverified', conversationResume='unverified')
    token = 'CF584-' + uuid.uuid4().hex[:12]
    prompt = ('Do not use tools. Remember this test token: ' + token + '. Inspect the attached image. '
              'Reply with only JSON containing token and colors (left-to-right English lowercase color names).')
    base = [binary, 'exec', '--json', '--skip-git-repo-check', '-c', 'features.multi_agent=false', '-c', 'features.shell_tool=false']
    first = run(base + ['--sandbox', 'read-only', '--image', str(image), '--', prompt], folder)
    result['first'] = first
    events = []
    for line in first['stdout'].splitlines():
        try:
            events.append(json.loads(line))
        except ValueError:
            pass
    thread = next((event.get('thread_id') for event in events if event.get('type') == 'thread.started'), None)
    texts = [event.get('item', {}).get('text', '') for event in events
             if first['exitCode'] == 0 and event.get('type') == 'item.completed'
             and event.get('item', {}).get('type') == 'agent_message']
    expected = ['red', 'green', 'blue']
    for text in texts:
        try:
            answer = json.loads(text)
            if answer.get('token') == token and answer.get('colors') == expected:
                result['imageInput'] = 'host_verified; native CLI recognizes deterministic image'
        except (ValueError, AttributeError):
            pass
    if first['exitCode'] == 0 and thread:
        result['threadId'] = thread
        second = run(base + ['resume', thread, 'Do not use tools. Reply only with the test token I gave in the previous message.'], folder)
        result['resume'] = second
        for line in second['stdout'].splitlines():
            try:
                event = json.loads(line)
            except ValueError:
                continue
            if (second['exitCode'] == 0 and event.get('type') == 'item.completed'
                    and event.get('item', {}).get('type') == 'agent_message'
                    and event['item'].get('text', '').strip() == token):
                result['conversationResume'] = 'host_verified; new CLI process resumed persisted conversation'
    (folder / 'model-result.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
    print(json.dumps({key: result[key] for key in ('imageInput', 'conversationResume')}))


if __name__ == '__main__':
    main()
