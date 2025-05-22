## Freezout Poker Docker image

### Run the Web server

To run the web server that serves the WASM client we need to know the WebSocket URL
the client uses to connect to the game server and pass it to the container. Use
`wss://host:port` if the server is using TLS encryption or `ws://host:port` if the
server is not using TLS, note that when TLS is disabled communication between client
and the game server is encrypted using the Noise Protocol.

For example if the game server is using TLS on port 9871 at `example.com`:

``` bash
$ docker run -p 80:80 -p 443:443 vincev/freezeout:latest  web wss://example.com:9871
Starting nginx server with web clients connecting to  wss://example.com:9871.
```

then use a browser to connect to the web server URL and load the client.

### Run the Poker server

To run the poker server use the `poker` command:

```bash
$ docker run -p 9871:9871 vincev/freezeout:latest poker
Starting Poker server
[2025-05-22T11:32:54.511Z INFO ] Listening on 0.0.0.0:9871 with 10 tables and 3 seats per table
[2025-05-22T11:32:54.511Z INFO ] Loading keypair /usr/share/freezeout/server.phrase
[2025-05-22T11:32:54.512Z INFO ] Loading database /usr/share/freezeout/game.db
[2025-05-22T11:32:54.513Z WARN ] TLS not enabled, using NOISE encryption
```

Note that we need to map the server listening port to allow clients to connect to it.
To use a different port we can use the `--port` option:

``` bash
$ docker run -p 9888:9888 vincev/freezeout:latest poker --port 9888
Starting Poker server
[2025-05-22T11:34:49.378Z INFO ] Listening on 0.0.0.0:9888 with 10 tables and 3 seats per table[2025-05-22T11:34:49.378Z INFO ] Loading keypair /usr/share/freezeout/server.phrase
[2025-05-22T11:34:49.378Z INFO ] Loading database /usr/share/freezeout/game.db
[2025-05-22T11:34:49.380Z WARN ] TLS not enabled, using NOISE encryption
```

By default the poker server runs with 10 tables and 3 seats per table, we can
override these values using the `--tables` and `--seats` options:

``` bash
$ docker run -p 9871:9871 vincev/freezeout:latest poker --tables 5 --seats 6
Starting Poker server
[2025-05-22T11:37:22.698Z INFO ] Listening on 0.0.0.0:9871 with 5 tables and 6 seats per table
[2025-05-22T11:37:22.698Z INFO ] Loading keypair /usr/share/freezeout/server.phrase
[2025-05-22T11:37:22.698Z INFO ] Loading database /usr/share/freezeout/game.db
[2025-05-22T11:37:22.699Z WARN ] TLS not enabled, using NOISE encryption
```

The server saves player chips into a local database, to persist the data across
containers runs we can use a volume and map the server data folder with the `-v`
option:

``` bash
$ docker run -p 9871:9871 -v freezeout:/usr/share/freezeout vincev/freezeout:latest poker
Starting Poker server
[2025-05-22T11:37:22.698Z INFO ] Listening on 0.0.0.0:9871 with 10 tables and 3 seats per table
[2025-05-22T11:37:22.698Z INFO ] Loading keypair /usr/share/freezeout/server.phrase
[2025-05-22T11:37:22.698Z INFO ] Loading database /usr/share/freezeout/game.db
[2025-05-22T11:37:22.699Z WARN ] TLS not enabled, using NOISE encryption
```

To enale TSS you need specify certificates to use:

``` bash
$ docker run -p 9871:9871 vincev/freezeout:latest poker --key-path privkey.pem --chain-path fullchain.pem
Starting Poker server
[2025-05-22T11:41:14.926Z INFO ] Listening on 0.0.0.0:9871 with 10 tables and 3 seats per table
[2025-05-22T11:41:14.926Z INFO ] Loading keypair /usr/share/freezeout/server.phrase
[2025-05-22T11:41:14.926Z INFO ] Loading database /usr/share/freezeout/game.db
[2025-05-22T11:41:14.930Z INFO ] Loaded TLS chain from fullchain.pem
[2025-05-22T11:41:14.930Z INFO ] Loaded TLS key   from privkey.pem
```

### Build the image

To build an image that contains the `freezeout` Poker server, and an `nginx` server
that serves the client UI WASM binary go to the repository root and run:

```bash
docker build -f docker/Dockerfile -t user/image:version .
```


