#!/usr/bin/env python

import asyncio
import json

message_seq = 1


async def message_receiver(reader):
    while True:
        data = await reader.readline()
        if not data:
            break
        print(f"<- {data.decode().strip()}\n")


async def jsonrpc_request(writer, method, params=None):
    global message_seq
    message_seq += 1
    data = {"jsonrpc": "2.0", "method": method, "params": params, "id": message_seq}
    message = json.dumps(data)
    writer.write((message + "\n").encode())
    await writer.drain()


async def jsonrpc_response(writer, message, result):
    data = {"jsonrpc": "2.0", "result": result, "id": message["id"]}
    message = json.dumps(data)
    writer.write((message + "\n").encode())
    await writer.drain()


async def user_input_handler(writer):
    while True:
        match await asyncio.to_thread(input):
            case "ping":
                await jsonrpc_request(writer, "ping")
            case "exit":
                break
            case "call:":
                plugin_id = int(input("enter plugin id: "))
                action = input("enter action: ")
                await jsonrpc_request(writer, "call_action", {"plugin_id": plugin_id, "action": json.loads(action)})
            case query:
                await jsonrpc_request(writer, "search", {"query": query})


async def request_handler(reader, writer):
    while True:
        data = await reader.readline()
        if not data:
            break

        print(f"-> {data.decode().strip()}")
        message = json.loads(data.decode().strip())
        match message:
            case {"method": "ping"}:
                await jsonrpc_response(writer, message, None)
            case {"method": "search", "params": {"query": query}}:
                await jsonrpc_response(
                    writer,
                    message,
                    [
                        {
                            "title": "Calculator",
                            "subtitle": f"A simple calculator app: {query}",
                            "icon": {"name": "calculator"},
                            "category": "Utility",
                            "actions": [
                                {"type": "Open", "path": "/usr/bin/calculator"},
                                {"type": "LaunchApp", "app_id": "calculator", "new_instance": True},
                            ],
                        },
                    ],
                )
            case {"method": "call_action"}:
                await jsonrpc_response(writer, message, None)


async def main():
    try:
        reader, writer = await asyncio.open_unix_connection("/run/user/1000/glimpsed.sock")
        plugin_reader, plugin_writer = await asyncio.open_unix_connection("/run/user/1000/glimpsed-plugins.sock")

        try:
            async with asyncio.TaskGroup() as tg:
                tg.create_task(message_receiver(reader))
                tg.create_task(user_input_handler(writer))
                tg.create_task(request_handler(plugin_reader, plugin_writer))
        except* ExceptionGroup as e:
            print(f"Error in task: {e}")

        writer.close()
        await writer.wait_closed()
    except Exception as e:
        print(f"Failed to connect: {e}")


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        pass
