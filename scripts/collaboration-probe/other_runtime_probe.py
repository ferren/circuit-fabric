"""Bounded native CLI probes using existing local sign-in; no EDA writes.

Only random test markers are sent. This is native capability evidence, separate
from CircuitFabric adapter and EDA integration. Run each runtime explicitly.
"""
import argparse
import datetime
import json
from pathlib import Path
import uuid

from model_probe import run
from runtime_probe import command, version_command


def events(stdout):
    for line in stdout.splitlines():
        try:
            yield json.loads(line)
        except ValueError:
            continue


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--runtime', choices=['claude', 'kimi', 'dsh'], required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--resume-result', type=Path, help='Resume only the observed Kimi probe session; do not repeat first call')
    args = parser.parse_args()
    folder = args.out.resolve()
    folder.mkdir(parents=True, exist_ok=True)
    token = 'CF584-' + uuid.uuid4().hex[:12]
    prompt = 'Do not use any tools or read files. Remember this test token and reply with only it: ' + token
    result = dict(schemaVersion=1, collectedAt=datetime.datetime.now(datetime.timezone.utc).isoformat(),
                  runtime=args.runtime, scope='native CLI only; no EDA integration; no interactive Q&A',
                  version=run(version_command(args.runtime), folder),
                  inference='unverified', conversationResume='unverified', testToken=token)
    result['executable'] = version_command(args.runtime)[:-1]
    result['firstCwd'] = str(folder)
    if args.resume_result:
        if args.runtime != 'kimi':
            parser.error('--resume-result is only supported for Kimi')
        current_version = result['version']
        result = json.loads(args.resume_result.read_text(encoding='utf-8'))
        if result['runtime'] != 'kimi':
            parser.error('source result is not a Kimi probe')
        token = result['testToken']
        result.setdefault('firstCwd', str(args.resume_result.resolve().parent))
        result['resumeVersion'] = current_version
        result['resumeCollectedAt'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
        result['executable'] = version_command('kimi')[:-1]
    if args.runtime == 'claude':
        session = str(uuid.uuid4())
        # Safe mode keeps the existing sign-in/model while disabling hooks,
        # skills and project customizations. Empty tools and strict MCP prevent
        # incidental engineering actions in this text-only probe.
        base = [command('claude'), '--safe-mode', '--print', '--output-format', 'stream-json',
                '--verbose', '--tools', '', '--strict-mcp-config', '--mcp-config', '{"mcpServers":{}}',
                '--disable-slash-commands', '--permission-mode', 'dontAsk']
        first = run(base + ['--session-id', session, prompt], folder)
        result['first'] = first
        answers = [event.get('result', '').strip() for event in events(first['stdout'])
                   if event.get('type') == 'result' and not event.get('is_error')]
        if first['exitCode'] == 0 and token in answers:
            result['inference'] = 'host_verified'
            result['sessionId'] = session
            second = run(base + ['--resume', session, 'Do not use tools. Reply only with the token from my previous message.'], folder)
            result['resume'] = second
            if second['exitCode'] == 0 and any(event.get('type') == 'result'
                    and not event.get('is_error') and event.get('result', '').strip() == token
                    for event in events(second['stdout'])):
                result['conversationResume'] = 'host_verified; new native process'
    elif args.runtime == 'kimi':
        base = [command('kimi'), '--output-format', 'stream-json']
        if not args.resume_result:
            result['first'] = run(base + ['--prompt', prompt], folder)
        # Keep the full native event stream; session identity must be observed
        # before resuming. Never select an unrelated user's latest session.
        native_events = list(events(result['first']['stdout']))
        if result['first']['exitCode'] == 0 and any(event.get('role') == 'assistant'
                and event.get('content', '').strip() == token for event in native_events):
            result['inference'] = 'host_verified'
            hint = next((event for event in native_events if event.get('type') == 'session.resume_hint'), {})
            session = hint.get('session_id')
            if session:
                result['sessionId'] = session
                # This switch is emitted by this binary in its machine-readable
                # resume hint. Do not execute the hint string as a shell command.
                switch = '-r' if hint.get('command', '').startswith('kimi -r ') else '--session'
                # Native Kimi binds a persisted session to its initial cwd.
                first_cwd = Path(result.get('firstCwd', str(args.resume_result.resolve().parent) if args.resume_result else str(folder)))
                second = run(base + [switch, session, '--prompt', 'Do not use tools. Reply only with the token from my previous message.'], first_cwd)
                result['resume'] = second
                if second['exitCode'] == 0 and any(event.get('role') == 'assistant'
                        and event.get('content', '').strip() == token for event in events(second['stdout'])):
                    result['conversationResume'] = 'host_verified; new native process'
    else:
        disabled = ['skill-filesystem', 'tool-skill', 'skill-badge', 'tool-bash', 'tool-pwsh',
                    'tool-jobs', 'tool-fs', 'tool-fs-search', 'tool-subagent-control',
                    'tool-subagent-list-agents', 'tool-subagent', 'tool-subagent-fork',
                    'tool-subagent-report', 'tool-workflow', 'tool-str-replace-editor',
                    'tool-web', 'tool-ralph', 'tool-goal', 'tool-todo']
        patch = folder / 'no-tools.patch.yml'
        patch.write_text(''.join('- ' + json.dumps(dict(id=name, disabled=True)) + '\n'
                                 for name in disabled), encoding='utf-8')
        # Preserve existing provider configuration; never print composed config
        # or copy credentials. Invoke the installed Node entry directly.
        base = version_command('dsh')[:-1]
        result['first'] = run(base + ['--profile', 'headless', '--patch', str(patch), prompt], folder)
        if result['first']['exitCode'] == 0 and result['first']['stdout'].strip() == token:
            result['inference'] = 'host_verified'
    if result['first']['exitCode'] != 0:
        result['inference'] = 'failed; see native exit code and error output'
    (folder / 'result.json').write_text(json.dumps(result, indent=2, ensure_ascii=False), encoding='utf-8')
    print(json.dumps({key: result[key] for key in ('runtime', 'inference', 'conversationResume')}))


if __name__ == '__main__':
    main()
