#!/bin/bash

if [ "$1" = "poker" ]; then
    echo "Starting Poker server"
    exec freezeout-server -a 0.0.0.0 --data-path /usr/share/freezeout "${@:2}"
elif [ "$1" = "web" ]; then
    if [ -z "$HOST" ]; then
        echo "Error: Environment variable 'HOST' is not defined."
        echo "This is the address the client uses to connect to the poker server."
        exit 1
    fi

    if [ -z "$PORT" ]; then
        PORT=9871
    elif ! [[ "$PORT" =~ ^[0-9]+$ ]] || [ "$PORT" -le 1024 ]; then
        echo "Error: Environment variable 'PORT' must be a number >= 1024"
        exit 1
    fi

    # Set the server host address for the Poker clients.
    sed -i "s|localhost:9871|${HOST}:${PORT}|g" /usr/share/nginx/html/index.html

    echo "Starting nginx server with web clients connecting to ${HOST}."
    exec nginx -g 'daemon off;'
elif [ -n "$1" ]; then
    # Run a user command
    exec "$@"
else
    echo "Specify 'poker' for the Poker server or 'web' for the web server."
    exit 1
fi
