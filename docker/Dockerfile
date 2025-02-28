FROM rust:latest as builder

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

# Copy the game server application from the builder image.
COPY --from=builder \
    /app/target/release/freezeout-server \
    /usr/local/bin/freezeout-server

# Copy the wasm client from the builder image.
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
