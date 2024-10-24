# Use a Rust base image
FROM rust:1-slim-bookworm

# Set the working directory
WORKDIR /src/

# Install necessary dependencies
RUN apt-get update && \
    apt-get install -y libssl-dev pkg-config && \
    apt-get clean

# Copy the source code into the container
COPY . .

# Build the application with verbose output
RUN CARGO_PROFILE_RELEASE_DEBUG=1 cargo build --release

# Copy the compiled binary to a more appropriate location
RUN cp target/release/data-migration /usr/local/bin/data-migration

# Ensure the binary is executable
RUN chmod +x /usr/local/bin/data-migration

# Set the entrypoint for the container
ENTRYPOINT ["/usr/local/bin/data-migration"]
