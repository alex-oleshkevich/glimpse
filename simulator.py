import asyncio


async def message_receiver(reader):
    while True:
        data = await reader.readline()
        if not data:
            break
        print(f"Received: {data.decode().strip()}")


async def main():
    try:
        reader, writer = await asyncio.open_unix_connection("/run/user/1000/glimpsed.sock")
        print("Connected to glimpsed socket")

        async with asyncio.TaskGroup() as tg:
            tg.create_task(message_receiver(reader))

            while True:
                message = await asyncio.to_thread(input, "Enter message to send (or 'exit' to quit): ")
                if message == "exit":
                    break
                writer.write(message.encode() + b"\n")
                await writer.drain()

        writer.close()
        await writer.wait_closed()
    except Exception as e:
        print(f"Failed to connect: {e}")


if __name__ == "__main__":
    asyncio.run(main())
