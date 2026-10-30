"""Probe installed Kimi ACP; optionally identify a test image in a new session."""
import argparse
import datetime
import json
from pathlib import Path
import queue
import subprocess
import threading
import time
import base64
import uuid
import hashlib

from runtime_probe import command


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--image', type=Path, help='Optional deterministic image inference in a new session')
    args = parser.parse_args()
    folder = args.out.resolve()
    folder.mkdir(parents=True, exist_ok=True)
    request = dict(jsonrpc='2.0', id=1, method='initialize', params=dict(
        protocolVersion=1, clientCapabilities=dict(
            fs=dict(readTextFile=False, writeTextFile=False), terminal=False),
        clientInfo=dict(name='cf584-capability-probe', version='1')))
    result = dict(schemaVersion=1, collectedAt=datetime.datetime.now(datetime.timezone.utc).isoformat(),
                  scope='native ACP initialization only; no session, inference, MCP or engineering calls',
                  request=request)
    messages = queue.Queue()
    with (folder / 'stderr.txt').open('w', encoding='utf-8') as err:
        proc = subprocess.Popen([command('kimi'), 'acp'], cwd=folder,
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=err,
            text=True, encoding='utf-8', errors='replace')
        result['childPid'] = proc.pid
        def read():
            for line in proc.stdout:
                messages.put(line)
        reader = threading.Thread(target=read, daemon=True)
        reader.start()
        try:
            proc.stdin.write(json.dumps(request) + '\n')
            proc.stdin.flush()
            line = messages.get(timeout=20)
            result['rawResponse'] = line
            result['response'] = json.loads(line)
            if args.image and 'result' in result['response']:
                trace = [result['response']]
                def rpc(method, params, request_id):
                    proc.stdin.write(json.dumps(dict(jsonrpc='2.0', id=request_id, method=method, params=params))+'\n')
                    proc.stdin.flush()
                    deadline = time.monotonic()+55
                    while time.monotonic() < deadline:
                        event = json.loads(messages.get(timeout=max(.1, deadline-time.monotonic())))
                        trace.append(event)
                        if 'method' in event and 'id' in event:
                            # Reject permissions and every client-side tool request.
                            reply = dict(jsonrpc='2.0', id=event['id'])
                            if event['method'] == 'session/request_permission':
                                reply['result'] = {'outcome':{'outcome':'cancelled'}}
                            else:
                                reply['error'] = {'code':-32601,'message':'Probe client provides no engineering tools'}
                            proc.stdin.write(json.dumps(reply)+'\n')
                            proc.stdin.flush()
                            trace.append(reply)
                        if event.get('id') == request_id and 'method' not in event:
                            return event
                    raise TimeoutError('ACP response deadline')
                try:
                    created = rpc('session/new', {'cwd':str(folder), 'mcpServers':[]}, 2)
                    result['created'] = created
                    session = created['result']['sessionId']
                    token = 'CF584-'+uuid.uuid4().hex[:12]
                    result['testToken'] = token
                    image_bytes = args.image.read_bytes()
                    result['fixture'] = dict(sha256=hashlib.sha256(image_bytes).hexdigest(),
                                             sizeBytes=len(image_bytes), mimeType='image/png')
                    result['scope'] = 'native ACP image inference in new test session; no EDA/MCP tools; permissions rejected'
                    result['promptResult'] = rpc('session/prompt', {'sessionId':session,'prompt':[
                        {'type':'text','text':'Do not use tools or read files. Reply only with JSON containing token='+token+' and colors (left-to-right lowercase English color names in the attached image).'},
                        {'type':'image','mimeType':'image/png','data':base64.b64encode(image_bytes).decode('ascii')}]}, 3)
                    texts=[]
                    for event in trace:
                        update=event.get('params',{}).get('update',{})
                        if update.get('sessionUpdate') == 'agent_message_chunk':
                            texts.append(update.get('content',{}).get('text',''))
                    result['agentText']=''.join(texts)
                    answer=json.loads(result['agentText'])
                    unexpected=[e['method'] for e in trace if e.get('method') and e.get('id') is not None]
                    result['clientToolRequests']=unexpected
                    result['imageVerified']=(answer.get('token')==token and answer.get('colors')==['red','green','blue']
                        and result['promptResult'].get('result',{}).get('stopReason')=='end_turn' and not unexpected)
                finally:
                    (folder/'rpc.jsonl').write_text(''.join(json.dumps(e,ensure_ascii=False)+'\n' for e in trace),encoding='utf-8')
        except (queue.Empty, ValueError, OSError, KeyError, TimeoutError) as error:
            result['failure'] = type(error).__name__ + ': ' + str(error)
        finally:
            # Persist the reply before cleanup so an OS cleanup failure cannot
            # erase the protocol evidence or the exact owned child PID.
            (folder / 'result.json').write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding='utf-8')
            proc.stdin.close()
            try:
                proc.wait(timeout=2)
            except subprocess.TimeoutExpired:
                pass
            if proc.poll() is None:
                stopped = subprocess.run(['taskkill', '/PID', str(proc.pid), '/T', '/F'],
                                         capture_output=True, timeout=10)
                result['cleanupExitCode'] = stopped.returncode
            proc.wait(timeout=10)
            reader.join(timeout=2)
            proc.stdout.close()
            result['childReaped'] = proc.poll() is not None
    (folder / 'result.json').write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding='utf-8')
    print(json.dumps({key: result[key] for key in
        ('collectedAt', 'scope', 'childPid', 'imageVerified', 'childReaped', 'failure') if key in result}))


if __name__ == '__main__':
    main()
