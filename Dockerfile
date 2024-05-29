# Use the official Rust image as the base image
FROM rust:latest

# Set the working directory inside the container
WORKDIR /usr/src/hpoint/network_websocket

# Copy the source code
COPY network_websocket/ /usr/src/hpoint/network_websocket
COPY hpoint_db_pg /usr/src/hpoint/hpoint_db_pg

# Build the project
RUN cargo build --release

ENTRYPOINT ["/usr/src/hpoint/network_websocket/target/release/network_websocket"]

CMD ["--config", "/usr/src/hpoint/network_websocket/config.yaml"]


