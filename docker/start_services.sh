#!/bin/bash

if [ -z "$HOST" ]; then
    echo "Error: Environment variable 'HOST' is not defined."
    exit 1
fi

if [ -z "$PORT" ]; then
    PORT=9871
elif ! [[ "$PORT" =~ ^[0-9]+$ ]] || [ "$PORT" -le 1024 ]; then
    echo "Error: Environment variable 'PORT' must be a number >= 1024"
    exit 1
fi

if [ -z "$TABLES" ]; then
    TABLES=10
elif ! [[ "$TABLES" =~ ^[0-9]+$ ]] || [ "$TABLES" -le 1 ]; then
    echo "Error: Environment variable 'TABLES' must be a number >= 1."
    exit 1
fi

if [ -z "$SEATS" ]; then
    SEATS=3
elif ! [[ "$SEATS" =~ ^[0-9]+$ ]] || [ "$SEATS" -lt 2 ] || [ "$SEATS" -gt 6 ]; then
    echo "Error: Environment variable 'SEATS' must be a number between 2 and 6."
    exit 1
fi

# Start Poker server in background.
echo "Starting Poker server on port ${PORT}"
echo "Poker server running with ${TABLES} tables and ${SEATS} seats per table."
freezeout-server -a 0.0.0.0 -p $PORT --tables $TABLES --seats $SEATS & pid=$!

# Wait for server startup.
sleep 1

# Check if Poker server has started up.
if ! kill -0 "$pid" 2>/dev/null; then
    echo "Poker server didn't start"
    exit 1
fi

# Set the server host address for the Poker clients.
sed -i "s|localhost:9871|${HOST}:${PORT}|g" /usr/share/nginx/html/index.html

echo "Starting nginx server with web clients connecting to ${HOST}."
nginx -g 'daemon off;'
