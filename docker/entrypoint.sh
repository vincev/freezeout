#!/bin/bash

check_url() {
    local url="$1"
    if [[ "$url" =~ ^wss?://[a-zA-Z0-9.-]+:[0-9]+$ ]]; then
        return 0
    else
        return 1
    fi
}

if [ "$1" = "poker" ]; then
    echo "Starting Poker server"
    exec freezeout-server -a 0.0.0.0 --data-path /usr/share/freezeout "${@:2}"
elif [ "$1" = "web" ]; then
    if [ -z "$2" ]; then
        echo "Missing server URL parameter."
        echo "This is the URL the client uses to connect to the poker server."
        echo "Example: wss://host:port or ws://host:port"
        exit 1
    fi

    if ! check_url "$2"; then
        echo "Error: Invalid URL, must be ws://host:port or wss://host:port"
        exit 1
    fi

    html_folder="/usr/share/nginx/html"
    app_folder="${html_folder}/freezeout"
    certs_folder="/usr/share/nginx/certs"

    # Delete old app folder in case is a local mount point
    rm -rf $app_folder

    # Create web folders
    mkdir -p $html_folder $app_folder $certs_folder

    # Copy the freezeout files to the nginx app folder
    cp /usr/share/nginx/app/freezeout/* $app_folder

    # Set the server url for the Poker clients.
    sed -i "s|ws://localhost:9871|${2}|g" ${app_folder}/index.html

    echo "Starting nginx server with web clients connecting to ${2}."
    exec nginx -g 'daemon off;'
elif [ -n "$1" ]; then
    # Run a user command
    exec "$@"
else
    echo "Specify 'poker' for the Poker server or 'web' for the web server."
    exit 1
fi
