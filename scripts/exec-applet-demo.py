#!/usr/bin/env python3
import argparse
import json
import select
import signal
import sys
import time


def send(message):
    print(json.dumps(message, separators=(",", ":")), flush=True)


def node(id, kind, props=None, children=None):
    return {"id": id, "type": kind, "props": props or {}, "children": children or []}


def context_text(placement, options):
    return json.dumps({"placement": placement, "options": options}, separators=(",", ":"))


def initial_commit(name, placement, options):
    rows = [
        node(5, "row", {"title": "Notify", "onActivate": True}),
        node(6, "row", {"title": "Copy text", "onActivate": True}),
        node(7, "row", {"title": "Open glimpse repo", "onActivate": True}),
        node(8, "row", {"title": "Lock screen", "onActivate": True}),
        node(9, "row", {"title": "Close popover", "onActivate": True}),
        node(10, "switchrow", {"title": "Enabled", "active": False, "onToggle": True}),
        node(11, "fader", {"value": 0.5, "maximum": 1.0, "onChange": True}),
        node(12, "entry", {"placeholder": "Type here", "value": "", "onChange": True, "onSubmit": True}),
    ]
    indicator = node(1, "indicator", {"icon": "applications-science-symbolic", "badge": "0", "onPress": True})
    popover = node(2, "popover", children=[
        node(3, "hero", {"title": "Exec demo", "subtitle": name}),
        node(4, "section", children=[node(13, "label", {"text": context_text(placement, options)}), *rows]),
        node(14, "footer", children=[node(15, "button", {"text": "Notify", "onClick": True})]),
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

    for count in (100, 300):
        dense = many_nodes_commit(count)
        sections = dense["ops"][1]["node"]["children"]
        assert 3 + len(sections) + sum(len(section["children"]) for section in sections) == count
        assert all(len(section["children"]) <= 200 for section in sections)
        for op in dense["ops"]:
            check_node(op["node"])


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
            7: {"t": "open-uri", "uri": "https://github.com/"},
            8: {"t": "session", "action": "lock"},
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
    line = sys.stdin.readline()
    if not line:
        return 0
    hello = json.loads(line)
    if hello.get("t") != "hello" or hello.get("v") != 1:
        return 1
    name = hello["name"]
    options = hello["options"]
    placement = hello["placement"]
    if mode == "garbage":
        print("not json", flush=True)
        return 0
    if mode == "huge-line":
        print("x" * (2 * 1024 * 1024), flush=True)
        return 0
    if mode == "many-nodes":
        send(many_nodes_commit(nodes))
        for _ in sys.stdin:
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
        if mode == "churn" and not select.select([sys.stdin], [], [], 0.1)[0]:
            tick += 1
            send({"t": "commit", "ops": [{"op": "set", "id": 13, "props": {"text": f"Tick {tick}"}}]})
            continue
        line = sys.stdin.readline()
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
    return run(args.mode, args.seconds, args.nodes)


if __name__ == "__main__":
    sys.exit(main())
