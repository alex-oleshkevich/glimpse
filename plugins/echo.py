import argparse
import asyncio
import json
import sys
# import gi


# def get_app_info():
#     app_info = Gio.app_info_get_all()
#     return [
#         {
#             "id": app.get_id(),
#             "title": app.get_name(),
#             "subtitle": app.get_description(),
#             "icon": {"name": app.get_icon().to_string()} if app.get_icon() else "applications-other",
#         }
#         for app in app_info
#     ]


async def run():
    reader = asyncio.StreamReader()
    protocol = asyncio.StreamReaderProtocol(reader)
    await asyncio.get_event_loop().connect_read_pipe(lambda: protocol, sys.stdin)
    while True:
        line = await reader.readline()
        if not line:
            break

        message = json.loads(line.decode("utf-8"))
        reply = json.dumps(
            {
                "jsonrpc": "2.0",
                "id": message["id"] + 1,
                "error": None,
                "result": {
                    "title": f"Reply {message['id']}",
                    "subtitle": "This is a reply to your search",
                    "category": "Apps",
                    "icon": {
                        "name": "computer",
                    },
                    "actions": [],
                },
            }
        )
        sys.stdout.write(reply + "\n")
        sys.stdout.flush()


async def main(args):
    await run()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Echo plugin for Glimpse")
    parser.add_argument(
        "--stdio",
        action="store_true",
        help="Use stdio for communication (default: True)",
    )
    args = parser.parse_args()

    try:
        asyncio.run(main(args))
    except KeyboardInterrupt:
        exit(0)
