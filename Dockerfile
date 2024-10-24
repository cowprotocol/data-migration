# Start from a minimal base image
FROM debian:buster-slim

# Set the working directory
WORKDIR /app

# Copy the binary from the `cross` build (adjust the path to your binary)
COPY target/x86_64-unknown-linux-gnu/release/data-migration /app/data-migration

# Ensure the binary is executable
RUN chmod +x /app/data-migration

# Set the binary as the entrypoint
ENTRYPOINT ["/app/data-migration"]
