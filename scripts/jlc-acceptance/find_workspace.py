import json

path = r'C:\Users\Ferren\AppData\Local\LCEDA-Pro\cache.x64.3\Preferences'
d = json.load(open(path, encoding='utf-8', errors='replace'))
hits = []

def walk(o, p=''):
    if isinstance(o, dict):
        for k, v in o.items():
            walk(v, p + '/' + str(k))
    elif isinstance(o, list):
        for i, v in enumerate(o):
            walk(v, p + '/' + str(i))
    elif isinstance(o, str):
        s = o.lower()
        if '\\' in o and ('jlc' in s or 'workspace' in s or 'project' in s):
            hits.append((p, o[:220]))

walk(d)
for p, v in hits[:40]:
    print(p, '=', v)
