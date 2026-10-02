FROM rust:slim

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
RUN cargo install cargo-chef --locked
RUN cargo install sccache --locked

# Install rustfmt and clippy components
RUN rustup component add rustfmt clippy

# Verify installations
RUN cargo chef --version && sccache --version && cargo fmt --version && cargo clippy --version

# Ensure cargo-installed binaries are on PATH for all container runs
ENV PATH=/usr/local/cargo/bin:$PATH

# Ensure sccache directory exists
RUN mkdir -p /root/.cache/sccache /root/.cargo/registry /root/.cargo/git

ENV RUSTC_WRAPPER=/usr/local/cargo/bin/sccache
WORKDIR /work

CMD ["bash"]
