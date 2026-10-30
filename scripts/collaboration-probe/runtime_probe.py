"""Bounded, foreground, local App Server protocol probe (no model call).

Usage: python scripts/collaboration-probe/runtime_probe.py --out artifacts/collaboration
This tests handshake/thread RPC only; never claims a conversation resume passed.
"""
import argparse
import datetime
import json
import os
from pathlib import Path
import queue
import shutil
import socket
import subprocess
import threading
import time


def command(name):
    path = shutil.which(name + (".cmd" if os.name == "nt" and name == "codex" else ""))
    if not path:
        raise FileNotFoundError(name)
    if os.name == "nt" and name == "codex":
        package = Path(path).resolve().parent / "node_modules" / "@openai" / "codex"
        native = list(package.glob("node_modules/@openai/codex-win32-*/vendor/*/bin/codex.exe"))
        if len(native) != 1:
            raise RuntimeError("Set PATH to the native codex.exe; refusing a Windows wrapper process")
        return str(native[0])
    return path


def version_command(name):
    """Use the installed Node entry directly so timeout owns the real process."""
    if os.name == "nt" and name == "dsh":
        wrapper = shutil.which("dsh.cmd")
        if wrapper:
            root = Path(wrapper).resolve().parent
            entry = root / "node_modules/@deepseek-ai/dsh/lib/bin.js"
            if entry.is_file():
                return [str(root / "node.exe"), str(entry), "--version"]
    return [command(name), "--version"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    result = {"schemaVersion": 1, "collectedAt": datetime.datetime.now(datetime.timezone.utc).isoformat(),
              "probeVersion": "1.1.0", "scope": "four runtime versions; local protocol; no inference; no EDA writes",
              "versions": {}, "bridge": {}, "conversationResume": "unverified",
              "questionAnswer": "unverified", "imageInput": "unverified"}
    for name in ("codex", "claude", "kimi", "dsh"):
        try:
            completed = subprocess.run(version_command(name), capture_output=True,
                                       text=True, encoding="utf-8", errors="replace", timeout=15)
            result["versions"][name] = {"exitCode": completed.returncode, "stdout": completed.stdout.strip()}
        except (OSError, RuntimeError, subprocess.TimeoutExpired) as exc:
            result["versions"][name] = {"error": str(exc)}
    for port in (49630, 49640):
        try:
            with socket.create_connection(("127.0.0.1", port), timeout=2):
                result["bridge"][str(port)] = "tcp reachable; executor identity unverified"
        except OSError as exc:
            result["bridge"][str(port)] = str(exc)
    trace = args.out / "codex-rpc.jsonl"
    home = args.out.resolve() / "codex-home"
    home.mkdir(exist_ok=True)
    env = dict(os.environ, CODEX_HOME=str(home))
    # Isolate configuration/history; authentication is intentionally not copied.
    with trace.open("w", encoding="utf-8") as log, (args.out / "codex-stderr.txt").open("w", encoding="utf-8") as err:
        proc = subprocess.Popen([command("codex"), "app-server", "--stdio"],
                                stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=err,
                                text=True, encoding="utf-8", env=env)
        result["probeChildPid"] = proc.pid
        messages = queue.Queue()

        def read():
            for line in proc.stdout:
                messages.put(line)
            messages.put(None)

        reader = threading.Thread(target=read)
        reader.start()

        def send(message):
            log.write(json.dumps({"direction": "send", "message": message}) + "\n")
            log.flush()
            proc.stdin.write(json.dumps(message) + "\n")
            proc.stdin.flush()

        def rpc(identifier, method, params):
            send({"id": identifier, "method": method, "params": params})
            deadline = time.monotonic() + 20
            while time.monotonic() < deadline:
                raw = messages.get(timeout=max(.01, deadline - time.monotonic()))
                if raw is None:
                    raise RuntimeError("App Server closed stdout")
                message = json.loads(raw)
                log.write(json.dumps({"direction": "recv", "message": message}) + "\n")
                log.flush()
                if message.get("id") == identifier and "method" not in message:
                    return message
                if "id" in message and "method" in message:
                    send({"id": message["id"], "error": {"code": -32601, "message": "Probe has no interactive handler"}})
            raise TimeoutError(method)

        try:
            result["initialize"] = rpc(1, "initialize", {"clientInfo": {"name": "cf-capability-probe", "version": "1.0.0"}, "capabilities": {"experimentalApi": True}})
            send({"method": "initialized", "params": {}})
            started = rpc(2, "thread/start", {"cwd": str(args.out.resolve()), "approvalPolicy": "never", "sandbox": "read-only"})
            result["threadStart"] = started
            thread_id = started.get("result", {}).get("thread", {}).get("id")
            if thread_id:
                result["emptyThreadResume"] = rpc(3, "thread/resume", {"threadId": thread_id})
        except (OSError, ValueError, RuntimeError, TimeoutError, queue.Empty) as exc:
            result["protocolError"] = type(exc).__name__ + ": " + str(exc)
        finally:
            # App Server can start helpers inheriting stdout. Closing stdin or
            # killing only the parent leaves the pipe reader blocked on Windows.
            # End this exact probe tree while its root PID is still live.
            if os.name == "nt" and proc.poll() is None:
                cleanup = subprocess.run(["taskkill", "/PID", str(proc.pid), "/T", "/F"],
                                         capture_output=True, timeout=10)
                result["cleanupExitCode"] = cleanup.returncode
            proc.stdin.close()
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()  # Exact probe child only; never terminate by executable name.
                proc.wait(timeout=5)
            reader.join(timeout=5)
            if reader.is_alive():
                raise RuntimeError("probe reader did not finish")
    (args.out / "result.json").write_text(json.dumps(result, indent=2, ensure_ascii=False), encoding="utf-8")
    print(json.dumps(result, ensure_ascii=False))


if __name__ == "__main__":
    main()
