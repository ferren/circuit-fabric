import json
import glob
import os

base = r'D:\Projects\Mojian\observations\inspect-history'
hist = sorted(glob.glob(os.path.join(base, '*.json')))
a = json.load(open(hist[0], encoding='utf-8'))['externalRefs']
b = json.load(open(hist[1], encoding='utf-8'))['externalRefs']

print('type a:', type(a).__name__, 'type b:', type(b).__name__)

def flat(x):
    # flatten to comparable items
    out = []
    if isinstance(x, dict):
        for k, v in x.items():
            out.append((k, json.dumps(v, sort_keys=True)))
    elif isinstance(x, list):
        for item in x:
            out.append(json.dumps(item, sort_keys=True))
    return out

fa = flat(a)
fb = flat(b)
sa = set(json.dumps(i, sort_keys=True) for i in fa)
sb = set(json.dumps(i, sort_keys=True) for i in fb)
print('only in 01:32 record:')
for d in sorted(sa - sb)[:10]:
    print('  ', d[:220])
print('only in 20:xx record:')
for d in sorted(sb - sa)[:10]:
    print('  ', d[:220])
