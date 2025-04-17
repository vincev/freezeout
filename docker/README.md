## Freezout Poker Docker image

### Build the image

This `Dockerfile` builds an image that contains the `freezeout` Poker server, the
`freezeout` client UI, and an `nginx` server that serves the client WASM binary.

To build the image go to the repository root and run:

```bash
docker build -f docker/Dockerfile -t freezeout:0.1.0 .
```

### Run the Web server

To run the web server that serves the WASM client we need to know the server host
address and port the client uses to connect to the server and pass it to the
container using the environment variable `HOST` and optionally `PORT` if we use a non
default port:

``` bash
$ docker run -e HOST=192.168.178.101 -p 80:80 freezeout:0.1.0  web
Starting nginx server with web clients connecting to 192.168.178.101.
```

then to run the UI client use a browser to connect to the web server URL, note that
this uses http for loading the WASM client, connections between the UI client and the
game server are encrypted using NOISE protocol.

### Run the Poker server

To run the poker server use the `poker` command:

```bash
$ docker run -p 9871:9871 freezeout:0.1.0 poker
Starting Poker server
[2025-04-06T18:16:02.948Z INFO ] Listening on 0.0.0.0:9871 with 10 tables and 3 seats per table
[2025-04-06T18:16:02.951Z INFO ] Writing keypair /usr/share/freezeout/server.phrase
[2025-04-06T18:16:02.951Z INFO ] Writing database /usr/share/freezeout/game.db
```

Note that we need to map the server listening port to allow clients to connect to it.
To use a different port we can use the `--port` option:

``` bash
$ docker run -p 9888:9888 freezeout:0.1.0 poker --port 9888
Starting Poker server
[2025-04-06T18:37:33.810Z INFO ] Listening on 0.0.0.0:9888 with 10 tables and 3 seats per table
[2025-04-06T18:37:33.813Z INFO ] Writing keypair /usr/share/freezeout/server.phrase
[2025-04-06T18:37:33.813Z INFO ] Writing database /usr/share/freezeout/game.db
```

By default the poker server runs with 10 tables and 3 seats per table, we can
override these values using the `--tables` and `--seats` options:

``` bash
$ docker run -p 9871:9871 freezeout:0.1.0 poker --tables 5 --seats 6
Starting Poker server
[2025-04-06T18:17:29.736Z INFO ] Listening on 0.0.0.0:9871 with 5 tables and 6 seats per table
[2025-04-06T18:17:29.740Z INFO ] Writing keypair /usr/share/freezeout/server.phrase
[2025-04-06T18:17:29.740Z INFO ] Writing database /usr/share/freezeout/game.db
```

The server saves player chips into a local database, to persist the data across
containers runs we can use a volume and map the server data folder with the `-v`
option:

``` bash
$ docker run -p 9871:9871 -v freezeout:/usr/share/freezeout freezeout:0.1.0 poker
Starting Poker server
[2025-04-06T18:30:29.906Z INFO ] Listening on 0.0.0.0:9871 with 10 tables and 3 seats per table
[2025-04-06T18:30:29.906Z INFO ] Writing keypair /usr/share/freezeout/server.phrase
[2025-04-06T18:30:29.906Z INFO ] Writing database /usr/share/freezeout/game.db
```

