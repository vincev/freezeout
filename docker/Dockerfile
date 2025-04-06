FROM rust:latest AS builder

# Install wasm target and trunk for trunk build.
RUN rustup target add wasm32-unknown-unknown
RUN cargo install trunk

WORKDIR /app
COPY . .

# Build the game server.
RUN cargo build --release --package freezeout-server

# Build the client wasm application.
WORKDIR /app/crates/gui
RUN trunk build --release

# Build runtime image
FROM nginx:latest

# Install tini to get ctrl-c
RUN apt-get update \
    && apt-get install -y tini \
    && rm -rf /var/lib/apt/lists/* /var/cache/apt/*


# Copy the game server application from the builder image.
COPY --from=builder \
    /app/target/release/freezeout-server \
    /usr/local/bin/freezeout-server

# Copy the wasm client from the builder image.
COPY --from=builder /app/crates/gui/dist /usr/share/nginx/html

# Copy the Nginx configuration file.
COPY ./docker/nginx.conf /etc/nginx/nginx.conf

# Copy the startup script and set permissions.
COPY ./docker/entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh

ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/entrypoint.sh"]
