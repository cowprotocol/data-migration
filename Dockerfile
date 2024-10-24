FROM docker.io/rust:1-slim-bookworm as cargo-build
WORKDIR /src/

# Install dependencies
RUN --mount=type=cache,target=/var/cache/apt,sharing=locked apt-get update && \
    apt-get install -y git libssl-dev pkg-config git

# Copy the binary from the `cross` build (adjust the path to your binary)
COPY target/x86_64-unknown-linux-gnu/release/data-migration /app/data-migration

# Ensure the binary is executable
RUN chmod +x /app/data-migration

# Set the binary as the entrypoint
ENTRYPOINT ["/app/data-migration"]
