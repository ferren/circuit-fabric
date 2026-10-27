import json
import sys
import glob
import os

base = r'D:\Projects\Mojian\observations'
hist = sorted(glob.glob(os.path.join(base, 'inspect-history', '*.json')))
records = []
for p in hist:
    with open(p, encoding='utf-8') as fh:
        records.append((os.path.basename(p), json.load(fh)))

print('inspect history count:', len(records))
refs = {}
for name, rec in records:
    snap = rec.get('snapshot', {})
    sh = snap.get('snapshot_hash')
    lh = snap.get('logical_hash')
    ext = rec.get('externalRefs', {})
    refs.setdefault(json.dumps(ext, sort_keys=True), []).append(name[:40])
    print(name[:44], sh[:23] if sh else None, lh[:23] if lh else None,
          'refs:', len(ext), 'inconclusive:', rec.get('inconclusive'))

print('distinct externalRefs sets:', len(refs))

# compare first and latest external ref maps in detail
if len(records) >= 2:
    for i in range(len(records) - 1):
        a = records[i][1].get('externalRefs', {})
        b = records[i + 1][1].get('externalRefs', {})
        sa = json.dumps(a, sort_keys=True)
        sb = json.dumps(b, sort_keys=True)
        if sa == sb:
            print(f'records[{i}] vs records[{i+1}]: externalRefs identical')
            continue
        print(f'records[{i}] vs records[{i+1}]: externalRefs DIFFER')
        ka = set(a.keys()) if isinstance(a, dict) else set()
        kb = set(b.keys()) if isinstance(b, dict) else set()
        for k in sorted(ka - kb):
            print('  only in first:', k)
        for k in sorted(kb - ka):
            print('  only in second:', k)
        for k in sorted(ka & kb):
            if json.dumps(a[k], sort_keys=True) != json.dumps(b[k], sort_keys=True):
                print('  value diff:', k, json.dumps(a[k], ensure_ascii=False), '->', json.dumps(b[k], ensure_ascii=False))

# which component triggered the components inconclusive face
latest = records[-1][1]
payload = latest.get('observedPayload', {})
for c in payload.get('components', []):
    if not c.get('primitiveId') or not c.get('designator') or c.get('name') is None:
        print('INCONCLUSIVE COMPONENT:', json.dumps(c, ensure_ascii=False)[:300])

# pinType evidence from real pins
pins_seen = 0
ptype_values = set()
for c in payload.get('components', []):
    for pin in c.get('pins', []):
        pins_seen += 1
        ptype_values.add(repr(pin.get('pinType')))
print('pins observed:', pins_seen, 'pinType values:', sorted(ptype_values)[:8])
