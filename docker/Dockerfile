FROM rust:latest as builder

RUN cargo install trunk && \
    apt-get update && \
    apt-get install -y musl-tools

WORKDIR /app
COPY . .

# Install musl and wasm targets
RUN rustup target add x86_64-unknown-linux-musl && \
    rustup target add wasm32-unknown-unknown

# Build the game server.
RUN cargo build \
    --release \
    --target x86_64-unknown-linux-musl \
    --package freezeout-server

# Build the client wasm application.
WORKDIR /app/crates/gui
RUN trunk build --release

# Build runtime image
FROM nginx:alpine

# Copy the game server application to the runtime.
COPY --from=builder \
    /app/target/x86_64-unknown-linux-musl/release/freezeout-server \
    /usr/local/bin/freezeout-server

# Copy the wasm client to the nginx folder.
COPY --from=builder /app/crates/gui/dist /usr/share/nginx/html

# Copy the Nginx configuration file.
COPY ./docker/nginx.conf /etc/nginx/nginx.conf

# Copy the startup script and set permissions.
COPY ./docker/start_services.sh /usr/local/bin/start_services.sh
RUN chmod +x /usr/local/bin/start_services.sh

# Expose the ports for Nginx and the poker server
EXPOSE 80
EXPOSE 9871

# Start the the Poker server and and the Nginx server
CMD ["/usr/local/bin/start_services.sh"]
