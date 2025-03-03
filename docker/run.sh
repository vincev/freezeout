#!/bin/bash

port=9871
tables=10
seats=3

usage() {
  echo "Usage: $0 --image <image> --host <hostname> [--port <port>] [--tables <tables>] [--seats <seats>]"
  exit 1
}

while [[ "$1" != "" ]]; do
  case $1 in
    --image )          shift
                       image=$1
                       ;;
    --host )           shift
                       host=$1
                       ;;
    --port )           shift
                       port=$1
                       ;;
    --tables )         shift
                       tables=$1
                       ;;
    --seats )          shift
                       seats=$1
                       ;;
    * )                usage
                       ;;
  esac
  shift
done

if [ -z "$image" ]; then
  echo "Error: missing --image parameter"
  usage
fi

if [ -z "$host" ]; then
  echo "Error: missing --host parameter"
  usage
fi

docker run \
       -e HOST="$host" \
       -e TABLES="$tables" \
       -e SEATS="$seats" \
       -e PORT="$port" \
       -p 80:80 \
       -p $port:$port \
       $image
