"""Real App Server request/answer and streaming interrupt probe, no EDA tools.

Uses existing runtime authentication. All children are collected before exit.
The answer token is generated only after a native input request arrives.
"""
import argparse
import datetime
import json
import os
from pathlib import Path
import queue
import re
import subprocess
import threading
import time
import tomllib
import uuid

from model_probe import run
from runtime_probe import command


class Session:
    def __init__(self, folder):
        self.log = (folder / 'rpc.jsonl').open('w', encoding='utf-8')
        self.stderr = (folder / 'stderr.txt').open('w', encoding='utf-8')
        argv = [command('codex'), '-c', 'features.multi_agent=false',
                '-c', 'features.shell_tool=false', '-c', 'features.code_mode=false',
                '-c', 'project_doc_max_bytes=0']
        config = Path(os.environ.get('CODEX_HOME', str(Path.home() / '.codex'))) / 'config.toml'
        if config.exists():
            settings = tomllib.loads(config.read_text(encoding='utf-8'))
            for name in settings.get('mcp_servers', {}):
                if not re.fullmatch(r'[A-Za-z0-9_-]+', name):
                    raise ValueError('MCP name needs an explicit safe config override')
                argv += ['-c', 'mcp_servers.' + name + '.enabled=false']
        self.proc = subprocess.Popen(argv + ['app-server', '--stdio'], cwd=folder,
                                     stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                     stderr=self.stderr, text=True, encoding='utf-8')
        self.queue = queue.Queue()
        self.received = []
        self.next_id = 0
        self.on_request = None
        self.reader = threading.Thread(target=self.read)
        self.reader.start()

    def read(self):
        for line in self.proc.stdout:
            self.queue.put(line)
        self.queue.put(None)

    def record(self, direction, message):
        self.log.write(json.dumps(dict(at=datetime.datetime.now(datetime.timezone.utc).isoformat(),
                                       direction=direction, message=message)) + '\n')
        self.log.flush()

    def send(self, message):
        self.record('send', message)
        self.proc.stdin.write(json.dumps(message) + '\n')
        self.proc.stdin.flush()

    def receive(self, deadline):
        line = self.queue.get(timeout=max(.01, deadline - time.monotonic()))
        if line is None:
            raise RuntimeError('App Server closed stdout')
        message = json.loads(line)
        self.record('recv', message)
        self.received.append(message)
        if 'id' in message and 'method' in message:
            if self.on_request:
                self.on_request(message)
            else:
                self.send(dict(id=message['id'], error=dict(code=-32601, message='Probe does not grant tool execution')))
        return message

    def rpc(self, method, params, timeout=25):
        self.next_id += 1
        identifier = self.next_id
        self.send(dict(id=identifier, method=method, params=params))
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            message = self.receive(deadline)
            if message.get('id') == identifier and 'method' not in message:
                if 'error' in message:
                    raise RuntimeError(method + ': ' + json.dumps(message['error']))
                return message['result']
        raise TimeoutError(method)

    def until(self, predicate, timeout=55, since=0):
        for message in self.received[since:]:
            if predicate(message):
                return message
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            message = self.receive(deadline)
            if predicate(message):
                return message
        raise TimeoutError('notification deadline')

    def close(self):
        if self.proc.poll() is None:
            if os.name == 'nt':
                cleanup = subprocess.run(['taskkill', '/PID', str(self.proc.pid), '/T', '/F'],
                                         capture_output=True, timeout=10)
                if cleanup.returncode != 0 and self.proc.poll() is None:
                    raise RuntimeError('probe child cleanup failed')
            else:
                self.proc.kill()
        self.proc.stdin.close()
        self.proc.wait(timeout=5)
        self.reader.join(timeout=5)
        self.stderr.close()
        self.log.close()
        if self.reader.is_alive():
            raise RuntimeError('probe reader did not finish')


def completed_for(thread_id, turn_id):
    return lambda message: (message.get('method') == 'turn/completed'
                            and message.get('params', {}).get('threadId') == thread_id
                            and message['params'].get('turn', {}).get('id') == turn_id)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    folder = args.out.resolve()
    folder.mkdir(parents=True, exist_ok=True)
    result = dict(schemaVersion=1, collectedAt=datetime.datetime.now(datetime.timezone.utc).isoformat(),
                  scope='native App Server; synthetic client answer; no EDA or engineering actions',
                  version=run([command('codex'), '--version'], folder),
                  questionAnswer='unverified', streamingCancel='unverified')
    session = Session(folder)
    try:
        result['initialize'] = session.rpc('initialize', dict(clientInfo=dict(name='cf-capability-probe', version='1.2.0'),
                                                             capabilities=dict(experimentalApi=True)))
        session.send(dict(method='initialized', params={}))
        started = session.rpc('thread/start', dict(cwd=str(folder), approvalPolicy='never',
                                                  sandbox='read-only', ephemeral=True,
                                                  baseInstructions='You are a capability test agent. Follow the test prompt; never read files, run commands, browse, or spawn agents.'))
        thread_id = started['thread']['id']
        result['threadId'] = thread_id
        result['model'] = started['model']
        answers = []

        def answer(message):
            if message['method'] != 'item/tool/requestUserInput':
                session.send(dict(id=message['id'], error=dict(code=-32601, message='Only test input requests supported')))
                return
            token = 'CF584-ANSWER-' + uuid.uuid4().hex[:12]
            questions = message['params']['questions']
            response = {question['id']: dict(answers=[token]) for question in questions}
            answers.append(dict(requestId=message['id'], token=token, questionIds=list(response)))
            session.send(dict(id=message['id'], result=dict(answers=response)))

        session.on_request = answer
        prompt = ('Call request_user_input once to ask one question with id probe_answer: what test token should I echo? '
                  'Do not guess the answer. After receiving the tool answer, reply with exactly the answer token and nothing else. '
                  'Do not use other tools. This is a protocol test, not a planning task.')
        start = session.rpc('turn/start', dict(threadId=thread_id, input=[dict(type='text', text=prompt)],
                                              collaborationMode=dict(mode='plan', settings=dict(model=started['model'], reasoning_effort='low',
                                                  developer_instructions='Perform the requested protocol test; ask via request_user_input and then echo the supplied token exactly. No plan or engineering actions.'))))
        finished = session.until(completed_for(thread_id, start['turn']['id']))
        result['questionTurn'] = finished['params']['turn']
        result['answers'] = answers
        texts = [item.get('text', '').strip() for message in session.received
                 if message.get('method') == 'item/completed'
                 and (item := message.get('params', {}).get('item', {})).get('type') == 'agentMessage']
        if len(answers) == 1 and answers[0]['token'] in texts and finished['params']['turn']['status'] == 'completed':
            result['questionAnswer'] = 'host_verified; native request, fresh client answer, exact echo'

        session.on_request = None
        offset = len(session.received)
        turn = session.rpc('turn/start', dict(threadId=thread_id,
            input=[dict(type='text', text='Do not use tools. Print integers from 1 through 100000, one per line. Start immediately.')],
            collaborationMode=dict(mode='default', settings=dict(model=started['model'], reasoning_effort='low',
                developer_instructions='This is a streaming cancellation test. Follow the user output request directly without tools.'))))
        turn_id = turn['turn']['id']
        delta = session.until(lambda message: message.get('method') == 'item/agentMessage/delta'
                              and message.get('params', {}).get('threadId') == thread_id
                              and message['params'].get('turnId') == turn_id
                              and bool(message['params'].get('delta')), since=offset)
        result['firstStreamingDelta'] = delta
        result['interruptResponse'] = session.rpc('turn/interrupt', dict(threadId=thread_id, turnId=turn_id))
        ended = session.until(completed_for(thread_id, turn_id), timeout=20, since=offset)
        result['cancelTurn'] = ended['params']['turn']
        if ended['params']['turn']['status'] == 'interrupted':
            result['streamingCancel'] = 'host_verified; interrupted after real agent text delta'
    except (OSError, RuntimeError, ValueError, TimeoutError, queue.Empty) as exc:
        result['error'] = type(exc).__name__ + ': ' + str(exc)
    finally:
        session.close()
        result['childCollected'] = True
        (folder / 'result.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
    print(json.dumps({key: result[key] for key in ('questionAnswer', 'streamingCancel')}))


if __name__ == '__main__':
    main()
