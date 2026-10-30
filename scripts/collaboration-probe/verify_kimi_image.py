"""Cross-check captured ACP image replies; never invokes a model."""
import argparse
import copy
import json
from pathlib import Path


def verify(result, events):
    def reply(request_id):
        matches = [e for e in events if e.get('id') == request_id and 'method' not in e]
        assert len(matches) == 1, 'missing/duplicate response'
        return matches[0]

    assert result.get('childReaped') is True, 'child cleanup unconfirmed'
    assert 'failure' not in result, 'probe failed'
    assert reply(1) == result['response'], 'initialize mismatch'
    assert reply(1)['result']['agentInfo']['version'] == '2.4.0', 'unexpected runtime'
    assert reply(2) == result['created'], 'session mismatch'
    session = reply(2)['result']['sessionId']
    chunks = []
    for e in events:
        assert not ('method' in e and 'id' in e), 'unexpected client request'
        if e.get('method') == 'session/update':
            assert e['params']['sessionId'] == session, 'cross-session update'
            update = e['params']['update']
            assert update.get('sessionUpdate') not in ('tool_call', 'tool_call_update'), 'tool activity'
            if update.get('sessionUpdate') == 'agent_message_chunk':
                chunks.append(update['content']['text'])
    assert reply(3) == result['promptResult'], 'completion mismatch'
    assert reply(3)['result']['stopReason'] == 'end_turn', 'incomplete response'
    answer = json.loads(''.join(chunks))
    assert answer['token'] == result['testToken'], 'token mismatch'
    assert answer['colors'] == ['red', 'green', 'blue'], 'wrong image answer'
    assert ''.join(chunks) == result['agentText'], 'answer summary mismatch'
    return session


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('folder', type=Path)
    parser.add_argument('--negative-controls', action='store_true')
    args = parser.parse_args()
    result = json.loads((args.folder / 'result.json').read_text(encoding='utf-8'))
    events = [json.loads(line) for line in (args.folder / 'rpc.jsonl').read_text(encoding='utf-8').splitlines()]
    session = verify(result, events)
    rejected = []
    if args.negative_controls:
        for name in ('token', 'session', 'completion', 'tool'):
            bad_result, bad_events = copy.deepcopy(result), copy.deepcopy(events)
            if name == 'token':
                bad_result['testToken'] = 'wrong-token'
            elif name == 'session':
                next(e for e in bad_events if e.get('method') == 'session/update')['params']['sessionId'] = 'wrong-session'
            elif name == 'completion':
                bad_events = [e for e in bad_events if e.get('id') != 3]
            else:
                bad_events.append(dict(jsonrpc='2.0', method='session/request_permission', id=999))
            try:
                verify(bad_result, bad_events)
            except AssertionError:
                rejected.append(name)
            else:
                raise AssertionError('tampered evidence accepted: ' + name)
    print(json.dumps(dict(verified=True, sessionId=session, negativeControlsRejected=rejected,
        scope='Captured reply consistency only; not desktop image/MCP/permission integration acceptance')))


if __name__ == '__main__':
    main()
