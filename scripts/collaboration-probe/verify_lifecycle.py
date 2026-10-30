"""Check collected host JSON against the native archive; never generates evidence."""
import argparse
import hashlib
import json
from pathlib import Path
import zipfile


def verify(folder):
    lifecycle = json.loads((folder / 'lifecycle-result.json').read_text(encoding='utf-8'))
    reopen = json.loads((folder / 'reopen-result.json').read_text(encoding='utf-8'))
    for result in (lifecycle, reopen):
        if result.get('failure') or any('error' in step for step in result['steps']):
            raise ValueError('Host probe recorded a failure')
        if not result.get('runtimeVersion') or result.get('schemaVersion') != 1:
            raise ValueError('Missing host version or unknown schema')
    project = lifecycle['testProjectUuid']
    pages = lifecycle['testPageUuids']
    if len(pages) != 2 or len(set(pages)) != 2 or reopen['fixture'] != project or reopen['pages'] != pages:
        raise ValueError('Fixture identities disagree')
    if any(reopen.get(key) is not True for key in ('stableAfterDocumentClose', 'stableAfterProjectSwitch', 'originalRestored')):
        raise ValueError('Reopen or restoration incomplete')
    for index, uuid in enumerate(pages):
        steps = {step['method']: step.get('value') for step in reopen['steps']}
        if steps[f'page{index}.close'] is not True or steps[f'page{index}.save'] is not True:
            raise ValueError('Missing successful close/save')
        before, after = steps[f'page{index}.beforeClose'], steps[f'page{index}.afterReopen']
        if before['uuid'] != uuid or after['uuid'] != uuid or before['name'] != after['name']:
            raise ValueError('Reopened identity mismatch')
    archive = folder / 'CF584-Lifecycle-20261030-sdk.epro2'
    with zipfile.ZipFile(archive) as zipped:
        if zipped.testzip() is not None:
            raise ValueError('Archive CRC failed')
        metadata = json.loads(zipped.read('project2.json'))
        if metadata['title'] != lifecycle['testName'] or metadata['editorVersion'] != lifecycle['runtimeVersion']:
            raise ValueError('Archive host version/title mismatch')
        documents = {}
        current = None
        for line in zipped.read(lifecycle['testName'] + '.epru').decode('utf-8').splitlines():
            if '||' not in line:
                continue
            header, body = line.split('||', 1)
            header, body = json.loads(header), json.loads(body.rstrip('|'))
            if header['type'] == 'DOCHEAD':
                current = body['uuid']
                documents[current] = dict(body)
            elif header['type'] == 'META' and current:
                documents[current]['meta'] = body
        for index, uuid in enumerate(pages):
            document = documents[uuid]
            if document['docType'] != 'SCH_PAGE' or document['meta']['title'] != ('Probe-A' if index == 0 else 'Probe-B'):
                raise ValueError('Native archive missing expected page')
        if documents[pages[0]]['meta']['schematic'] != documents[pages[1]]['meta']['schematic']:
            raise ValueError('Fixture pages are in different schematics')
    return dict(schemaVersion=1, runtimeVersion=lifecycle['runtimeVersion'], projectUuid=project,
                pageUuids=pages, archiveSha256=hashlib.sha256(archive.read_bytes()).hexdigest(),
                nativeArchiveMatches=True, scope='local save, document close/reopen, project switch; not restart/cloud sync')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('folder', type=Path)
    args = parser.parse_args()
    print(json.dumps(verify(args.folder), indent=2))
