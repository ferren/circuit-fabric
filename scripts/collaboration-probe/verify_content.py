"""Cross-check raw host evidence; this is not a substitute for host execution."""
import argparse
import copy
import json
from pathlib import Path

FIXTURE = 'acf0e4b3437356162b1742564a56cd8942fdca179333f9958e04a76cf63d83c0'
PAGES = ['0b6e010013c4d426', '112ce6c2685aa137']
MARKERS = ['CF584-CONTENT-A-20261030', 'CF584-CONTENT-B-20261030']


def require(value, message):
    if not value:
        raise ValueError(message)


def verify(write, restart, dirty):
    for data, expected_write in [(write, True), (restart, False)]:
        require(data.get('passed') is True and not data.get('failure'), 'host probe failed')
        require(data['fixture'] == FIXTURE and data['pages'] == PAGES, 'fixture identities')
        require(data['write'] is expected_write, 'write scope')
        steps = {s['method']: s for s in data['steps']}
        require(steps['project']['value']['uuid'] == FIXTURE, 'project read')
        for i, page in enumerate(PAGES):
            require(steps[f'{i}.identity']['value']['uuid'] == page, 'active page identity')
            key = f'{i}.reopened' if expected_write else f'{i}.read'
            texts = steps[key]['value']
            require([t['content'] for t in texts] == [MARKERS[i]], 'distinct saved content')
            if expected_write:
                require(steps[f'{i}.save']['value'] is True and steps[f'{i}.close']['value'] is True,
                        'save/close result')
        require(all(not s.get('error') for s in data['steps']), 'step error')
    require(not dirty.get('failure') and dirty['closeReturned'] is True, 'dirty close')
    marker = dirty['marker']
    require(marker in [t['content'] for t in dirty['beforeClose']], 'unsaved marker observed')
    require(marker not in [t['content'] for t in dirty['after']], 'unsaved marker discarded')
    require([t['content'] for t in dirty['after']] == [MARKERS[0]], 'saved baseline preserved')
    return dict(contentAcrossRestart=True, unsavedCloseDiscards=True,
                saveRetainCancel='not provided by tested closeDocument',
                archiveImport='blocked by host external-interaction permission',
                multiInstanceRouting='unverified', stageGate='not passed')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--dir', type=Path, required=True)
    args = parser.parse_args()
    values = [json.loads((args.dir / name).read_text(encoding='utf-8-sig')) for name in
              ['content-write-result.json', 'content-after-restart.json', 'dirty-result.json']]
    summary = verify(*values)
    for mutation in ['page', 'content', 'dirty']:
        altered = copy.deepcopy(values)
        if mutation == 'page':
            next(s for s in altered[1]['steps'] if s['method'] == '0.identity')['value']['uuid'] = PAGES[1]
        elif mutation == 'content':
            next(s for s in altered[1]['steps'] if s['method'] == '1.read')['value'][0]['content'] = MARKERS[0]
        else:
            altered[2]['beforeClose'] = []
        try:
            verify(*altered)
        except ValueError:
            continue
        raise RuntimeError('Evidence tampering accepted: ' + mutation)
    summary['negativeControlsRejected'] = 3
    (args.dir / 'verified-content-summary.json').write_text(json.dumps(summary, indent=2), encoding='utf-8')
    print(json.dumps(summary))


if __name__ == '__main__':
    main()
