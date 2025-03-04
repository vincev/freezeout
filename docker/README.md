## Freezout Poker Docker image

### Build the image

This `Dockerfile` builds an image that contains the `freezeout` Poker server, the
`freezeout` client UI, and an `nginx` server that serves the client WASM binary.

To build the image go to the repository root and run:

```bash
docker build -f docker/Dockerfile -t freezeout:0.1.0 .
```

### Run the container

To run the `freezeout` and `nginx` servers you need to know the server host
address that the client will use to connect to the server, let's say that the
address is `192.168.178.74`, then from the repository root you can start the
container using the `run.sh` script:

```bash
$ ./docker/run.sh --host 192.168.178.74 --image freezeout:0.1.0 
```

### Run the UI client

To run the UI client use a browser to connect to the URL `http://192.168.178.74`
(note this uses http, connections between the UI client and the game server are
encrypted using NOISE protocol).

### Configure number of tables and seats

By default the poker server runs with 10 tables and 3 seats per table, to
configure the number of tables you can use the `--tables` option:

```bash
$ ./docker/run.sh --host 192.168.178.74 --image freezeout:0.1.0 --tables 30
```

The `--seats` option configures the number of seats per table, this can be a
number between 2 and 6, to run the server for two players games:

```bash
$ ./docker/run.sh --host 192.168.178.74 --image freezeout:0.1.0 --seats 2
```

### Configure listening port

To `--port` option configures the `freezeout` server port, to use port 9800:

```bash
$ ./docker/run.sh --host 192.168.178.74 --image freezeout:0.1.0 --port 9800
```
