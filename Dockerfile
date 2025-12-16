
# Multi-stage build: build the release binary, then copy into a minimal runtime image
FROM rust:slim AS builder
WORKDIR /usr/src/ekphos

# Copy manifest first to cache dependencies layer
COPY Cargo.toml ./
# Replace with the full source and build the real binary

COPY . .
RUN cargo build --release


## Runtime image (SSH server with forced ekphos CLI on login)
## Use the same (newer) Debian family as the builder so glibc versions match
FROM debian:trixie-slim

# Install ssh server and ca-certificates
RUN apt-get update && apt-get install -y ca-certificates openssh-server && rm -rf /var/lib/apt/lists/*

# Create ekphos user and ssh runtime dirs
RUN useradd -m -s /bin/bash ekphos && \
	mkdir -p /var/run/sshd /home/ekphos/.ssh /config /data && \
	chown -R ekphos:ekphos /home/ekphos /config /data

WORKDIR /app

# Copy the compiled binary from the builder stage
COPY --from=builder /usr/src/ekphos/target/release/ekphos /app/ekphos

# Copy bundled themes (so the image still has shipped themes available)
COPY --from=builder /usr/src/ekphos/themes /app/themes

# Add a small startup script that will (if provided) install
# `/config/authorized_keys` into the ekphos user's .ssh/authorized_keys
RUN printf '%s\n' "#!/bin/sh" \
	"set -e" \
	"if [ -f /config/authorized_keys ]; then" \
	"  mkdir -p /home/ekphos/.ssh" \
	"  cp /config/authorized_keys /home/ekphos/.ssh/authorized_keys" \
	"  chown -R ekphos:ekphos /home/ekphos/.ssh" \
	"  chmod 700 /home/ekphos/.ssh" \
	"  chmod 600 /home/ekphos/.ssh/authorized_keys" \
	"fi" \
	"exec /usr/sbin/sshd -D" \
	> /usr/local/bin/start-sshd.sh && chmod +x /usr/local/bin/start-sshd.sh

# Configure sshd: disable root login, require pubkey auth, and force
# the ekphos binary to run when the `ekphos` user logs in.
RUN sed -i 's/^#Port 22/Port 22/' /etc/ssh/sshd_config || true && \
	echo 'PermitRootLogin no' >> /etc/ssh/sshd_config && \
	echo 'PasswordAuthentication no' >> /etc/ssh/sshd_config && \
	echo 'PubkeyAuthentication yes' >> /etc/ssh/sshd_config && \
	echo 'AuthorizedKeysFile .ssh/authorized_keys' >> /etc/ssh/sshd_config && \
	echo '' >> /etc/ssh/sshd_config && \
	echo 'Match User ekphos' >> /etc/ssh/sshd_config && \
	echo '    ForceCommand /app/ekphos' >> /etc/ssh/sshd_config && \
	echo '    PermitTTY yes' >> /etc/ssh/sshd_config && \
	echo '    AllowTcpForwarding no' >> /etc/ssh/sshd_config

# Expose SSH port and preserve volumes
EXPOSE 22
VOLUME ["/config", "/data"]

ENV RUST_LOG=info
CMD ["/usr/local/bin/start-sshd.sh"]

