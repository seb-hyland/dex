"""Ask a real language server what `dex.` completes to in a checkout."""
import json, subprocess, sys, os, threading

root = sys.argv[1]
lsp_root = sys.argv[2] if len(sys.argv) > 2 else root
main = os.path.join(root, "main.py")
text = open(main).read()
# Complete right after `dex.`
line_idx = next(i for i, l in enumerate(text.splitlines()) if l.strip().endswith("dex."))
char = len(text.splitlines()[line_idx])

p = subprocess.Popen(["basedpyright-langserver", "--stdio"],
                     stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)

def send(msg):
    body = json.dumps(msg).encode()
    p.stdin.write(f"Content-Length: {len(body)}\r\n\r\n".encode() + body)
    p.stdin.flush()

def recv():
    headers = {}
    while True:
        line = p.stdout.readline().decode()
        if line in ("\r\n", "\n", ""):
            break
        k, _, v = line.partition(":")
        headers[k.strip().lower()] = v.strip()
    n = int(headers.get("content-length", 0))
    return json.loads(p.stdout.read(n)) if n else None

send({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
    "processId":os.getpid(),"rootUri":f"file://{lsp_root}",
    "workspaceFolders":[{"uri":f"file://{lsp_root}","name":"root"}],
    "capabilities":{}}})
while True:
    m = recv()
    if m and m.get("id") == 1: break
send({"jsonrpc":"2.0","method":"initialized","params":{}})
send({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{
    "uri":f"file://{main}","languageId":"python","version":1,"text":text}}})

send({"jsonrpc":"2.0","id":2,"method":"textDocument/completion","params":{
    "textDocument":{"uri":f"file://{main}"},
    "position":{"line":line_idx,"character":char}}})

import time
deadline = time.time() + 20
while time.time() < deadline:
    m = recv()
    if m and m.get("id") == 2:
        items = (m.get("result") or {}).get("items", []) if isinstance(m.get("result"), dict) else (m.get("result") or [])
        labels = [i["label"] for i in items]
        real = [l for l in labels if not l.startswith("_")]
        print(f"total={len(labels)} non-dunder={len(real)}")
        print("sample:", sorted(real)[:12])
        break
else:
    print("no completion response")
p.kill()
