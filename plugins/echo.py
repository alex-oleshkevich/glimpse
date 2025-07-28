import asyncio
import json
import sys


async def stdio_reader():
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


async def main():
    await stdio_reader()


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        exit(0)
