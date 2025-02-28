## Freezout Poker Docker image

This `Dockerfile` builds an image that contains the `freezeout` Poker server, the
`freezeout` client UI, and an `nginx` server that serves the client WASM binary.

To build the image go to the repository root and run:

```bash
docker build -f docker/Dockerfile -t freezeout:0.1.0 .
```

To run the `freezeout` and `nginx` servers you need to know the server host address
that the client will use to connect to the server, let's say that the address is
`192.168.178.74`, then to run the servers container run the following command:

```bash
docker run -e SERVER_HOST="192.168.178.74" -p 80:80 -p 9871:9871 freezeout:0.1.0
```

If the servers start successfully then use a browser to load and run the client at
the URL `http://192.168.178.74`.
