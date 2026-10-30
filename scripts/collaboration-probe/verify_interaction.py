"""Check a collected native trace; this validator never manufactures evidence."""
import argparse
import copy
import json
from pathlib import Path

if not __debug__:
    raise RuntimeError('Run this evidence validator without Python -O; assertions are required')


def verify(records):
    def message(record):
        return record['message']

    questions = [(index, message(record)) for index, record in enumerate(records)
                 if record['direction'] == 'recv'
                 and message(record).get('method') == 'item/tool/requestUserInput']
    assert len(questions) == 1, 'expected exactly one native input request'
    request_index, request = questions[0]
    params = request['params']
    assert [question['id'] for question in params['questions']] == ['probe_answer']
    replies = [(index, message(record)) for index, record in enumerate(records)
               if index > request_index and record['direction'] == 'send'
               and message(record).get('id') == request['id'] and 'result' in message(record)]
    assert len(replies) == 1, 'request must have one matching response'
    reply_index, reply = replies[0]
    token = reply['result']['answers']['probe_answer']['answers'][0]
    assert token.startswith('CF584-ANSWER-') and len(token) > 13
    assert token not in json.dumps(records[:reply_index]), 'answer was leaked before client response'
    question_completed = [message(record)['params']['turn'] for record in records[reply_index + 1:]
                          if record['direction'] == 'recv' and message(record).get('method') == 'turn/completed'
                          and message(record)['params']['threadId'] == params['threadId']
                          and message(record)['params']['turn']['id'] == params['turnId']]
    assert len(question_completed) == 1 and question_completed[0]['status'] == 'completed'
    assert any(item.get('type') == 'agentMessage' and item.get('text', '').strip() == token
               for item in question_completed[0]['items']), 'agent did not echo supplied answer'

    interrupts = [(index, message(record)) for index, record in enumerate(records)
                  if record['direction'] == 'send' and message(record).get('method') == 'turn/interrupt']
    assert len(interrupts) == 1, 'expected one interrupt request'
    interrupt_index, interrupt = interrupts[0]
    target = interrupt['params']
    assert target['threadId'] == params['threadId']
    assert target['turnId'] != params['turnId'], 'cancel must target the streaming test turn'
    assert any(record['direction'] == 'recv'
               and message(record).get('method') == 'item/agentMessage/delta'
               and message(record)['params'].get('threadId') == target['threadId']
               and message(record)['params'].get('turnId') == target['turnId']
               and message(record)['params'].get('delta')
               for record in records[:interrupt_index]), 'no real streaming delta before interrupt'
    assert any(record['direction'] == 'recv' and message(record).get('id') == interrupt['id']
               and message(record).get('result') == {}
               for record in records[interrupt_index + 1:]), 'interrupt RPC did not succeed'
    assert any(record['direction'] == 'recv' and message(record).get('method') == 'turn/completed'
               and message(record)['params']['threadId'] == target['threadId']
               and message(record)['params']['turn']['id'] == target['turnId']
               and message(record)['params']['turn']['status'] == 'interrupted'
               for record in records[interrupt_index + 1:]), 'target turn was not interrupted'
    return dict(questionAnswer='verified against trace', streamingCancel='verified against trace',
                threadId=params['threadId'])


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('trace', type=Path)
    parser.add_argument('--negative-controls', action='store_true')
    args = parser.parse_args()
    records = [json.loads(line) for line in args.trace.read_text(encoding='utf-8').splitlines()]
    summary = verify(records)
    if args.negative_controls:
        def reject(mutated):
            try:
                verify(mutated)
            except AssertionError:
                return
            raise AssertionError('tampered trace was accepted')

        wrong_turn = copy.deepcopy(records)
        next(record for record in wrong_turn if record['message'].get('method') == 'turn/interrupt')['message']['params']['turnId'] = 'different-turn'
        reject(wrong_turn)
        missing_delta = [record for record in records if record['message'].get('method') != 'item/agentMessage/delta']
        reject(missing_delta)
        wrong_answer = copy.deepcopy(records)
        request = next(record['message'] for record in records if record['message'].get('method') == 'item/tool/requestUserInput')
        reply = next(record for record in wrong_answer if record['direction'] == 'send'
                     and record['message'].get('id') == request['id'] and 'result' in record['message'])
        reply['message']['id'] = 'wrong-request'
        reject(wrong_answer)
        leaked = copy.deepcopy(records)
        token = reply['message']['result']['answers']['probe_answer']['answers'][0]
        leaked[0]['message']['leakedAnswer'] = token
        reject(leaked)
        summary['negativeControlsPassed'] = 4
    print(json.dumps(summary))
