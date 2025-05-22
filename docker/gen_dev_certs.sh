# Generate development SSL certificates.
#!/bin/bash

certs_folder="/usr/share/nginx/certs"
mkdir -p $certs_folder

openssl req -x509 \
        -nodes \
        -days 365 \
        -newkey rsa:2048 \
        -keyout ${certs_folder}/privkey.pem \
        -out ${certs_folder}/fullchain.pem \
        -subj "/C=US/ST=State/L=City/O=Development/CN=localhost"

