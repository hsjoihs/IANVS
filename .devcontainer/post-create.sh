#!/bin/bash
set -eux

# Install rustup + Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain none
. "${HOME}/.cargo/env"
echo 'source "${HOME}/.cargo/env"' >> "${HOME}/.bashrc"
cd /workspace/app && rustup show
