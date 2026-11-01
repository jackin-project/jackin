import asyncio, sys

LISTEN_HOST, LISTEN_PORT, SOCK = sys.argv[1], int(sys.argv[2]), sys.argv[3]

async def pump(r, w):
    try:
        while data := await r.read(65536):
            w.write(data)
            await w.drain()
    except (ConnectionError, asyncio.CancelledError):
        pass
    finally:
        try:
            w.write_eof()
        except Exception:
            pass

async def handle(cr, cw):
    sr, sw = await asyncio.open_unix_connection(SOCK)
    await asyncio.gather(pump(cr, sw), pump(sr, cw))
    for w in (cw, sw):
        w.close()

async def main():
    srv = await asyncio.start_server(handle, LISTEN_HOST, LISTEN_PORT)
    async with srv:
        await srv.serve_forever()

asyncio.run(main())
