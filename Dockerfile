# Stage 1: Build the Rust binaries
FROM docker.io/rust:1-slim-bookworm as cargo-build
WORKDIR /src/

# Install dependencies
RUN --mount=type=cache,target=/var/cache/apt,sharing=locked apt-get update && \
    apt-get install -y git libssl-dev pkg-config

# Copy the source code into the container
COPY . .

# Build the application
RUN --mount=type=cache,target=/usr/local/cargo/registry --mount=type=cache,target=/src/target \
    CARGO_PROFILE_RELEASE_DEBUG=1 cargo build --release

# Stage 2: Create an intermediate image for dependencies
FROM docker.io/debian:bookworm-slim as intermediate
RUN --mount=type=cache,target=/var/cache/apt,sharing=locked apt-get update && \
    apt-get install -y ca-certificates tini gettext-base && \
    apt-get clean

# Stage 3: Create the final image
FROM intermediate as final
RUN apt-get update && \
    apt-get install -y build-essential cmake git zlib1g-dev libelf-dev libdw-dev libboost-dev libboost-iostreams-dev libboost-program-options-dev libboost-system-dev libboost-filesystem-dev libunwind-dev libzstd-dev git
COPY --from=cargo-build /src/target/release/data-migration /usr/local/bin/data-migration

# Ensure the binary is executable
RUN chmod +x /usr/local/bin/data-migration

# Set the entrypoint for the container
ENTRYPOINT ["/usr/local/bin/data-migration"]
