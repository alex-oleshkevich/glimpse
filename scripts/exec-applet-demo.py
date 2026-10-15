#!/usr/bin/env python3
import argparse
import json
import os
import select
import signal
import socket
import sys
import time

incoming = sys.stdin
outgoing = sys.stdout


def connect():
    global incoming, outgoing
    path = os.environ.get("GLIMPSE_APPLET_SOCKET")
    if not path:
        return True
    connection = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    try:
        connection.connect(path)
    except (FileNotFoundError, ConnectionRefusedError):
        return False
    incoming = connection.makefile("r", encoding="utf-8", newline="\n")
    outgoing = connection.makefile("w", encoding="utf-8", newline="\n")
    return True


def write(text):
    print(text, file=outgoing, flush=True)


def send(message):
    write(json.dumps(message, separators=(",", ":")))


def node(id, kind, props=None, children=None):
    return {"id": id, "type": kind, "props": props or {}, "children": children or []}


def context_text(placement, options):
    return json.dumps({"placement": placement, "options": options}, separators=(",", ":"))


def initial_commit(name, placement, options):
    rows = [
        node(5, "actionrow", {"title": "Notify", "onActivate": True}),
        node(6, "actionrow", {"title": "Copy text", "onActivate": True}),
        node(7, "actionrow", {"title": "Open glimpse repo", "onActivate": True}),
        node(8, "actionrow", {"title": "Launch missing app (error)", "onActivate": True}),
        node(9, "actionrow", {"title": "Close popover", "onActivate": True}),
        node(10, "togglerow", {"title": "Enabled", "active": False, "onToggle": True}),
        node(11, "sliderrow", {"title": "Level", "value": 0.5, "max": 1.0, "onChange": True}),
        node(12, "entry", {"placeholder": "Type here", "value": "", "onChange": True, "onSubmit": True}),
    ]
    indicator = node(1, "indicator", {"icon": "applications-science-symbolic", "badge": "0", "tooltip": "Exec demo: right-click to count, left-click to open", "onPress": True})
    popover = node(2, "popover", {"title": "Exec demo", "subtitle": name}, children=[
        node(4, "section", children=[node(13, "label", {"text": context_text(placement, options)}), *rows]),
        node(14, "box", children=[node(15, "button", {"text": "Notify", "onClick": True})]),
    ])
    return {"t": "commit", "ops": [
        {"op": "insert", "parent": 0, "node": indicator, "before": None},
        {"op": "insert", "parent": 0, "node": popover, "before": None},
    ]}


def many_nodes_commit(count=300):
    sections = []
    next_id = 3
    remaining = count - 3
    while remaining:
        rows = min(200, remaining - 1)
        sections.append(node(next_id, "section", children=[
            node(next_id + item + 1, "row", {"title": "Row"})
            for item in range(rows)
        ]))
        next_id += rows + 1
        remaining -= rows + 1
    return {"t": "commit", "ops": [
        {"op": "insert", "parent": 0, "node": node(1, "indicator", {"icon": "applications-science-symbolic"}), "before": None},
        {"op": "insert", "parent": 0, "node": node(2, "popover", children=sections), "before": None},
    ]}


def self_test():
    messages = [{"t": "hello", "v": 1}, initial_commit("Demo", {}, {})]
    lines = [json.dumps(message, separators=(",", ":")) for message in messages]
    hello, commit = (json.loads(line) for line in lines)
    assert set(hello) == {"t", "v"} and hello == {"t": "hello", "v": 1}
    assert set(commit) == {"t", "ops"} and commit["t"] == "commit"

    def check_node(item):
        assert set(item) == {"id", "type", "props", "children"}
        assert isinstance(item["id"], int) and isinstance(item["type"], str)
        assert isinstance(item["props"], dict) and isinstance(item["children"], list)
        for child in item["children"]:
            check_node(child)

    for op in commit["ops"]:
        assert set(op) == {"op", "parent", "node", "before"} and op["op"] == "insert" and op["parent"] == 0
        assert op["before"] is None
        check_node(op["node"])

    for count in (4, 100, 300):
        dense = many_nodes_commit(count)
        sections = dense["ops"][1]["node"]["children"]
        assert 3 + len(sections) + sum(len(section["children"]) for section in sections) == count
        assert all(len(section["children"]) <= 200 for section in sections)
        for op in dense["ops"]:
            check_node(op["node"])

    socket_self_test()


def socket_self_test():
    import subprocess
    import tempfile
    from pathlib import Path

    hello = {"t": "hello", "v": 1, "name": "Demo", "state": "/tmp", "options": {}, "placement": {"position": "bottom", "output": "test"}}
    command = [sys.executable, str(Path(__file__).resolve())]
    with tempfile.TemporaryDirectory(prefix="glimpse-demo-") as directory:
        path = str(Path(directory) / "applet.sock")
        env = {**os.environ, "GLIMPSE_APPLET_SOCKET": path}
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as listener:
            listener.bind(path)
            listener.listen()
            listener.settimeout(3)
            for mode in (None, "garbage", "huge-line", "flood", "no-read", "exit-before-hello", "crash-after", "many-nodes", "churn", "bad-host-hello"):
                args = [] if mode in (None, "bad-host-hello") else ["--mode", mode]
                if mode == "crash-after":
                    args.append("0.05")
                process = subprocess.Popen(command + args, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
                try:
                    connection, _ = listener.accept()
                    with connection:
                        connection.settimeout(3)
                        with connection.makefile("rw", encoding="utf-8", newline="\n") as stream:
                            if mode == "exit-before-hello":
                                assert stream.readline() == ""
                                assert process.wait(timeout=3) == 3
                                continue
                            assert json.loads(stream.readline()) == {"t": "hello", "v": 1}
                            stream.write(json.dumps({**hello, "v": 2} if mode == "bad-host-hello" else hello) + "\n")
                            stream.flush()
                            if mode == "bad-host-hello":
                                assert stream.readline() == ""
                                assert process.wait(timeout=3) == 1
                                continue
                            if mode == "garbage":
                                assert stream.readline() == "not json\n"
                            elif mode == "huge-line":
                                assert len(stream.readline()) == 2 * 1024 * 1024 + 1
                            elif mode == "flood":
                                assert all(json.loads(stream.readline()) == {"t": "commit", "ops": []} for _ in range(500))
                            else:
                                commit = json.loads(stream.readline())
                                assert commit["t"] == "commit"
                                if mode == "no-read":
                                    assert json.loads(stream.readline()) == {"t": "commit", "ops": []}
                                    process.terminate()
                                elif mode == "crash-after":
                                    assert stream.readline() == ""
                                    assert process.wait(timeout=3) == 1
                                elif mode == "churn":
                                    assert json.loads(stream.readline())["ops"][0]["props"] == {"text": "Tick 1"}
                                elif mode is None:
                                    assert commit == initial_commit("Demo", hello["placement"], {})
                                    cases = [
                                        ({"id": 1, "name": "onPress", "args": [3]}, {"t": "commit", "ops": [{"op": "set", "id": 1, "props": {"badge": "1"}}]}),
                                        ({"id": 5, "name": "onActivate"}, {"t": "notify", "summary": "Exec demo", "body": "Notification requested"}),
                                        ({"id": 6, "name": "onActivate"}, {"t": "copy", "text": "Exec demo"}),
                                        ({"id": 7, "name": "onActivate"}, {"t": "open-uri", "uri": "https://github.com/alex-oleshkevich/glimpse"}),
                                        ({"id": 8, "name": "onActivate"}, {"t": "launch", "id": "org.example.GlimpseExecDemo.Missing.desktop"}),
                                        ({"id": 9, "name": "onActivate"}, {"t": "close-popover"}),
                                        ({"id": 15, "name": "onClick"}, {"t": "notify", "summary": "Exec demo", "body": "Footer clicked"}),
                                    ]
                                    for id, name, value in ((10, "onToggle", True), (11, "onChange", 0.75), (12, "onChange", "hello"), (12, "onSubmit", "hello")):
                                        cases.append(({"id": id, "name": name, "args": [value], "seq": 7}, {"t": "commit", "ops": [{"op": "set", "id": id, "props": {"active" if id == 10 else "value": value}, "seq": 7}]}))
                                    for event, expected in cases:
                                        stream.write(json.dumps({"t": "event", **event}) + "\n")
                                        stream.flush()
                                        assert json.loads(stream.readline()) == expected
                                    for message in ({"t": "options", "options": {"example": True}}, {"t": "placement", "placement": {"position": "top", "output": "other"}}):
                                        hello.update({key: value for key, value in message.items() if key != "t"})
                                        stream.write(json.dumps(message) + "\n")
                                        stream.flush()
                                        assert json.loads(stream.readline())["ops"][0]["props"] == {"text": context_text(hello["placement"], hello["options"])}
                        connection.shutdown(socket.SHUT_RDWR)
                    if mode not in ("no-read", "crash-after"):
                        assert process.wait(timeout=3) == 0
                    assert process.stdout.read() == b""
                finally:
                    if process.poll() is None:
                        process.kill()
                    process.wait(timeout=3)
                    process.stdout.close()
                    process.stderr.close()
        for path in (str(Path(directory) / "missing.sock"), path):
            result = subprocess.run(command, env={**env, "GLIMPSE_APPLET_SOCKET": path}, capture_output=True, timeout=3)
            assert result.returncode == 0 and not result.stdout and not result.stderr


def handle_event(message, clicks):
    id = message.get("id")
    name = message.get("name")
    args = message.get("args", [])
    print(json.dumps(message, separators=(",", ":")), file=sys.stderr, flush=True)
    if id == 1 and name == "onPress":
        clicks += 1
        send({"t": "commit", "ops": [{"op": "set", "id": 1, "props": {"badge": str(clicks)}}]})
    elif name == "onActivate" or (id == 15 and name == "onClick"):
        requests = {
            5: {"t": "notify", "summary": "Exec demo", "body": "Notification requested"},
            6: {"t": "copy", "text": "Exec demo"},
            7: {"t": "open-uri", "uri": "https://github.com/alex-oleshkevich/glimpse"},
            8: {"t": "launch", "id": "org.example.GlimpseExecDemo.Missing.desktop"},
            9: {"t": "close-popover"},
            15: {"t": "notify", "summary": "Exec demo", "body": "Footer clicked"},
        }
        if id in requests:
            send(requests[id])
    elif id in (10, 11, 12) and name in ("onToggle", "onChange", "onSubmit") and args:
        prop = "active" if id == 10 else "value"
        op = {"op": "set", "id": id, "props": {prop: args[0]}}
        if message.get("seq") is not None:
            op["seq"] = message["seq"]
        send({"t": "commit", "ops": [op]})
    return clicks


def run(mode, crash_after, nodes):
    if mode == "exit-before-hello":
        return 3
    send({"t": "hello", "v": 1})
    line = incoming.readline()
    if not line:
        return 0
    hello = json.loads(line)
    if hello.get("t") != "hello" or hello.get("v") != 1:
        return 1
    name = hello["name"]
    options = hello["options"]
    placement = hello["placement"]
    if mode == "garbage":
        write("not json")
        return 0
    if mode == "huge-line":
        write("x" * (2 * 1024 * 1024))
        return 0
    if mode == "many-nodes":
        send(many_nodes_commit(nodes))
        for _ in incoming:
            pass
        return 0
    if mode == "flood":
        for _ in range(500):
            send({"t": "commit", "ops": []})
        return 0
    send(initial_commit(name, placement, options))
    if mode == "no-read":
        while True:
            time.sleep(1)
            send({"t": "commit", "ops": []})
    if crash_after is not None:
        signal.signal(signal.SIGALRM, lambda *_: sys.exit(1))
        signal.setitimer(signal.ITIMER_REAL, crash_after)
    clicks = 0
    tick = 0
    while True:
        if mode == "churn" and not select.select([incoming], [], [], 0.1)[0]:
            tick += 1
            send({"t": "commit", "ops": [{"op": "set", "id": 13, "props": {"text": f"Tick {tick}"}}]})
            continue
        line = incoming.readline()
        if not line:
            return 0
        message = json.loads(line)
        kind = message.get("t")
        if kind == "placement":
            placement = message["placement"]
        elif kind == "options":
            options = message["options"]
        elif kind == "event":
            clicks = handle_event(message, clicks)
            continue
        else:
            continue
        send({"t": "commit", "ops": [{"op": "set", "id": 13, "props": {"text": context_text(placement, options)}}]})


def main():
    parser = argparse.ArgumentParser(description="Protocol-speaking exec applet demo")
    parser.add_argument("--mode", choices=("garbage", "huge-line", "flood", "no-read", "exit-before-hello", "crash-after", "many-nodes", "churn"))
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--nodes", type=int, default=300)
    parser.add_argument("seconds", nargs="?", type=float, metavar="SECONDS")
    args = parser.parse_args()
    if args.mode == "crash-after" and args.seconds is None:
        parser.error("--mode crash-after requires SECONDS")
    if args.mode != "crash-after" and args.seconds is not None:
        parser.error("SECONDS requires --mode crash-after")
    if args.seconds is not None and args.seconds <= 0:
        parser.error("SECONDS must be positive")
    if not 4 <= args.nodes <= 300:
        parser.error("--nodes must be between 4 and 300")
    if args.self_test:
        self_test()
        return 0
    if not connect():
        return 0
    try:
        return run(args.mode, args.seconds, args.nodes)
    except (BrokenPipeError, ConnectionResetError):
        return 0


if __name__ == "__main__":
    sys.exit(main())
