#!/bin/sh

# Check if the SERVER_HOST environment variable is defined
if [ -z "$SERVER_HOST" ]; then
  echo "Error: Environment variable 'SERVER_HOST' is not defined."
  exit 1
fi

# Set the server host address for the Poker client.
sed -i "s|localhost:9871|${SERVER_HOST}:9871|g" /usr/share/nginx/html/index.html

# Start Poker server.
freezeout-server -a 0.0.0.0 &
nginx -g 'daemon off;'
