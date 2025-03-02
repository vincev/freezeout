## Freezout Poker Docker image

### Build the image

This `Dockerfile` builds an image that contains the `freezeout` Poker server, the
`freezeout` client UI, and an `nginx` server that serves the client WASM binary.

To build the image go to the repository root and run:

```bash
docker build -f docker/Dockerfile -t freezeout:0.1.0 .
```

### Run the container

To run the `freezeout` and `nginx` servers you need to know the server host address
that the client will use to connect to the server, let's say that the address is
`192.168.178.74`, then to run the servers container run the following command:

```bash
docker run -e HOST="192.168.178.74" -p 80:80 -p 9871:9871 freezeout:0.1.0
```

The above command maps the ports for the `nginx` and `freezeout`
servers and sets the host used by all clients.

### Connect the client

If the servers start successfully then use a browser to load and run the client at
the URL `http://192.168.178.74`.


### Configure number of tables and seats

By default the poker server runs with 10 tables and 3 seats per table,
to configure the number of tables use the `TABLES` variable:

```bash
docker run -e HOST="192.168.178.74" -e TABLES=20 -p 80:80 -p 9871:9871 freezeout:0.1.0
```

To configure the number of seats per table use the `SEATS` variable,
the number of seats must be a number between 2 and 6:

```bash
docker run -e HOST="192.168.178.74" -e SEATS=2 -p 80:80 -p 9871:9871 freezeout:0.1.0
```

### Configure listening port

To configure the `freezeout` server port use the environment variable
`PORT` and change the port mapping when running the container, for
example to use port 9800:

```bash
docker run -e HOST="192.168.178.74" -p 80:80 -e PORT=9800 -p 9800:9800 freezeout:0.1.0
```
