FROM rust:1.70-slim

# Install system dependencies needed by native crates
RUN apt-get update \
  && apt-get install -y --no-install-recommends \
    build-essential \
    pkg-config \
    libasound2-dev \
    libssl-dev \
    ca-certificates \
    curl \
    git \
  && rm -rf /var/lib/apt/lists/*

# Install cargo-chef and sccache for fast builds
RUN cargo install cargo-chef --locked || true
RUN cargo install sccache --locked || true

# Ensure sccache directory exists
RUN mkdir -p /root/.cache/sccache /root/.cargo/registry /root/.cargo/git

ENV RUSTC_WRAPPER=/usr/local/cargo/bin/sccache
WORKDIR /work

CMD ["bash"]
