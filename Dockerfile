# ============================================================
# NXR DATABASE — DOCKER IMAGE
# ============================================================
# Menggunakan glibc-based image karena binary Rust
# di-compile di host Ubuntu (glibc).
# ============================================================

FROM ubuntu:24.04

LABEL maintainer="NXR Team"
LABEL description="NXR AI-native database — Vector + Graph + KV engine"
LABEL version="0.1.0"

# Runtime dependencies
RUN apt-get update -qq && \
    apt-get install -y -qq ca-certificates tzdata netcat-openbsd && \
    rm -rf /var/lib/apt/lists/*

# Buat user non-root untuk keamanan
RUN groupadd --system nxr && useradd --system -g nxr -d /var/nxr-db -M -s /usr/sbin/nologin nxr

# Struktur direktori database
RUN mkdir -p /var/nxr-db/{vectors/segments,graph,kv/cold,wal,indexes,snapshots,logs} && \
    chown -R nxr:nxr /var/nxr-db

# Copy binary hasil build dari host
COPY --chown=nxr:nxr target/release/nxrd /usr/local/bin/nxrd

# Copy Go CLI jika ada (opsional)
COPY tools/nxr-cli/nxr /tmp/nxr-cli
RUN if [ -f /tmp/nxr-cli ]; then cp /tmp/nxr-cli /usr/local/bin/nxr && chown nxr:nxr /usr/local/bin/nxr; fi; rm -f /tmp/nxr-cli

# Copy config default
COPY --chown=nxr:nxr nxr-db/config.toml /var/nxr-db/config.toml

# Python SDK wheel
COPY --chown=nxr:nxr sdk/python/ /opt/nxr-sdk/

# Port default
EXPOSE 9643

# Volume data
VOLUME ["/var/nxr-db"]

# Health check
HEALTHCHECK --interval=30s --timeout=5s --start-period=3s --retries=3 \
    CMD nc -z 127.0.0.1 9643 || exit 1

# User non-root
USER nxr

WORKDIR /var/nxr-db

ENTRYPOINT ["nxrd"]
CMD ["--db-path", "/var/nxr-db", "--bind", "0.0.0.0:9643"]
