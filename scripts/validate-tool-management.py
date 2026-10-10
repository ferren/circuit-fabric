"""Foreground Windows native UI inspection with isolated, non-secret fixtures.

Build with cargo build -p circuitfabric-desktop --features native-ui, then run
this script. JSON commands on stdin drive only the recorded child window.
Examples: {"click":[300,300]}, {"text":"first"}, {"capture":"skills"},
{"resize":[1100,760]}, {"wheel":-5,"at":[600,500]}, {"quit":true}.
The process is owned by this foreground script and closed before it exits.
"""
import ctypes
from ctypes import wintypes
import json
import os
from pathlib import Path
import subprocess
import stat
import sys
import time
from PIL import ImageGrab

ROOT = Path(__file__).resolve().parents[1]
ARTIFACTS = ROOT / 'target' / 'tool-management-native'
ARTIFACTS.mkdir(parents=True, exist_ok=True)
DATA = ARTIFACTS / 'profile'
settings_path = DATA / 'CircuitFabric' / 'runtime.json'
settings_path.parent.mkdir(parents=True, exist_ok=True)
skills = []
servers = []
for index in range(35):
    resource_id = f'skill-{index:02}'
    folder = ARTIFACTS / resource_id
    folder.mkdir(exist_ok=True)
    text = f'---\nname: {resource_id}\ndescription: Native validation skill {index}\n---\n'
    text += ('Actual skill instruction and long details for scroll validation.\n' * (120 if index == 0 else 1))
    (folder / 'SKILL.md').write_text(text, encoding='utf-8')
    skills.append({'id':resource_id, 'path':str(folder / 'SKILL.md'), 'enabled':True})
for resource_id in ['first', 'second']:
    servers.append({'id':resource_id,'display_name':f'Local {resource_id}','command':sys.executable,
        'args':[str(ROOT / 'crates/circuitfabric-codex-runtime/tests/mcp_fixture.py'),str(ARTIFACTS / f'{resource_id}.calls')],
        'environment_variables':[],'enabled':True})
settings = {'providers':[{'id':'local','name':'Native fixture','kind':'open_ai_compatible',
    'base_url':'http://127.0.0.1:49000/v1','model':'fixture','api_key_environment_variable':'CF_UNUSED_FIXTURE_KEY',
    'native_vision':False,'enabled':True}], 'default_provider_id':'local',
    'tools':{'authorized_skill_ids':['skill-00','skill-01'],'authorized_mcp_server_ids':[]},
    'catalog':{'skills':skills,'mcp_servers':servers,'removed_bundled_servers':['typesafe-jev']},
    'global_preferences':{'language':'simplified_chinese','data_directory':str(ARTIFACTS / 'workspace-data')}}
settings['codex'] = {'command':'codex','working_directory':str(ARTIFACTS), 'model':None,'api_key_environment_variable':'CF_UNUSED_FIXTURE_KEY'}
settings['bridge'] = {'listen_address':'127.0.0.1:49630'}
if '--reuse' not in sys.argv:
    settings_path.write_text(json.dumps(settings,ensure_ascii=False),encoding='utf-8')

user32 = ctypes.windll.user32
user32.SetProcessDPIAware()
user32.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
user32.GetWindowRect.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.RECT)]
user32.SetForegroundWindow.argtypes = [wintypes.HWND]
user32.MoveWindow.argtypes = [wintypes.HWND,ctypes.c_int,ctypes.c_int,ctypes.c_int,ctypes.c_int,wintypes.BOOL]
user32.PostMessageW.argtypes = [wintypes.HWND,wintypes.UINT,wintypes.WPARAM,wintypes.LPARAM]
user32.GetForegroundWindow.restype = wintypes.HWND
user32.ShowWindow.argtypes = [wintypes.HWND,ctypes.c_int]
user32.BringWindowToTop.argtypes = [wintypes.HWND]
user32.GetWindowTextW.argtypes = [wintypes.HWND,wintypes.LPWSTR,ctypes.c_int]
enum_callback = ctypes.WINFUNCTYPE(wintypes.BOOL,wintypes.HWND,wintypes.LPARAM)

def find_window(pid):
    found = []
    @enum_callback
    def callback(hwnd, _):
        owner = wintypes.DWORD()
        user32.GetWindowThreadProcessId(hwnd,ctypes.byref(owner))
        if owner.value == pid and user32.IsWindowVisible(hwnd):
            title = ctypes.create_unicode_buffer(256)
            user32.GetWindowTextW(hwnd,title,256)
            if title.value.startswith('CircuitFabric'):
                found.append(hwnd)
        return True
    user32.EnumWindows(callback,0)
    return found[0] if found else None

def rectangle(hwnd):
    rect = wintypes.RECT()
    user32.GetWindowRect(hwnd,ctypes.byref(rect))
    return rect.left, rect.top, rect.right, rect.bottom

def focus(hwnd):
    foreground = user32.GetForegroundWindow()
    foreground_thread = user32.GetWindowThreadProcessId(foreground, None)
    own_thread = ctypes.windll.kernel32.GetCurrentThreadId()
    target_thread = user32.GetWindowThreadProcessId(hwnd, None)
    attached = bool(foreground_thread and foreground_thread != own_thread and user32.AttachThreadInput(own_thread,foreground_thread,True))
    target_attached = bool(target_thread != own_thread and target_thread != foreground_thread and user32.AttachThreadInput(own_thread,target_thread,True))
    try:
        user32.ShowWindow(hwnd,9)
        user32.BringWindowToTop(hwnd)
        user32.SetForegroundWindow(hwnd)
    finally:
        if attached: user32.AttachThreadInput(own_thread,foreground_thread,False)
        if target_attached: user32.AttachThreadInput(own_thread,target_thread,False)
    time.sleep(0.2)
    if user32.GetForegroundWindow() != hwnd:
        raise RuntimeError('cannot activate owned desktop window; no input sent')

def key(code, up=False):
    user32.keybd_event(code,0,2 if up else 0,0)

def click(hwnd, xy):
    left,top,_,_ = rectangle(hwnd)
    point = wintypes.POINT(left+xy[0],top+xy[1])
    user32.ScreenToClient(hwnd,ctypes.byref(point))
    packed = (point.y << 16) | point.x
    user32.PostMessageW(hwnd,0x0200,0,packed)
    user32.PostMessageW(hwnd,0x0201,1,packed)
    user32.PostMessageW(hwnd,0x0202,0,packed)

env = dict(os.environ, APPDATA=str(DATA))
log = (ARTIFACTS/'desktop.log').open('w',encoding='utf-8')
child = subprocess.Popen([str(ROOT/'target/debug/circuitfabric-desktop.exe')],cwd=ROOT,env=env,stdout=log,stderr=log)
try:
    hwnd = None
    for _ in range(150):
        hwnd = find_window(child.pid)
        if hwnd: break
        if child.poll() is not None: raise RuntimeError('desktop exited; inspect desktop.log')
        time.sleep(0.1)
    if not hwnd: raise RuntimeError('desktop window did not appear')
    user32.MoveWindow(hwnd,40,40,2600,1600,True)
    time.sleep(1)
    hwnd = find_window(child.pid)
    if not hwnd: raise RuntimeError('desktop has no visible window after startup')
    user32.MoveWindow(hwnd,40,40,2600,1600,True)
    print(json.dumps({'pid':child.pid,'bounds':rectangle(hwnd),'settings':str(settings_path)}),flush=True)
    for line in sys.stdin:
        command = json.loads(line)
        if command.get('quit'): break
        hwnd = find_window(child.pid)
        if not hwnd: raise RuntimeError('owned window closed')
        if 'readonly' in command:
            settings_path.chmod(stat.S_IREAD if command['readonly'] else stat.S_IREAD | stat.S_IWRITE)
        if 'resize' in command:
            user32.MoveWindow(hwnd,40,40,*command['resize'],True)
        if 'click' in command: click(hwnd,command['click'])
        if 'text' in command:
            focus(hwnd)
            key(0x11);key(0x41);key(0x41,True);key(0x11,True)
            for char in command['text']:
                user32.PostMessageW(hwnd,0x0102,ord(char),0)
        if 'keys' in command:
            focus(hwnd)
            for code in command['keys']: key(code)
            for code in reversed(command['keys']): key(code,True)
        if 'wheel' in command:
            left,top,_,_ = rectangle(hwnd)
            x,y = command.get('at',[900,500])
            user32.PostMessageW(hwnd,0x020A,((command['wheel']*120)&0xffff)<<16,((top+y)<<16)|(left+x))
        time.sleep(command.get('wait',0.5))
        if 'capture' in command:
            hwnd = find_window(child.pid)
            if not hwnd: raise RuntimeError('owned window closed before screenshot')
            focus(hwnd)
            time.sleep(0.5)
            path = ARTIFACTS / (command['capture']+'.png')
            ImageGrab.grab(bbox=rectangle(hwnd)).save(path)
            print(json.dumps({'capture':str(path)}),flush=True)
        print(json.dumps({'ok':True,'running':child.poll() is None}),flush=True)
finally:
    settings_path.chmod(stat.S_IREAD | stat.S_IWRITE)
    hwnd = find_window(child.pid)
    if hwnd: user32.PostMessageW(hwnd,0x0010,0,0)
    try: child.wait(timeout=5)
    except subprocess.TimeoutExpired: child.terminate();child.wait(timeout=5)
    log.close()
    print('native inspection child closed',flush=True)
