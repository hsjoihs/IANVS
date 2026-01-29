FROM mcr.microsoft.com/devcontainers/base:ubuntu-24.04

ARG USERNAME=vscode
WORKDIR /workspace

# Basic tools
RUN apt-get update && \
  apt-get install -y --no-install-recommends \
  xz-utils \
  ca-certificates \
  curl \
  git \
  dos2unix \
  ripgrep && \
  rm -rf /var/lib/apt/lists/*

# Install Codex
RUN wget https://github.com/openai/codex/releases/download/rust-v0.92.0/codex-x86_64-unknown-linux-gnu.tar.gz -O /tmp/codex.tar.gz && \
  tar -xvf /tmp/codex.tar.gz -C /usr/local/bin/ && \
  rm /tmp/codex.tar.gz && \
  mv /usr/local/bin/codex-x86_64-unknown-linux-gnu /usr/local/bin/codex && \
  chmod +x /usr/local/bin/codex

# Used to persist bash history. For details see: https://code.visualstudio.com/remote/advancedcontainers/persist-bash-history
RUN SNIPPET="export PROMPT_COMMAND='history -a' && export HISTFILE=/commandhistory/.bash_history" \
  && mkdir /commandhistory \
  && touch /commandhistory/.bash_history \
  && chown -R $USERNAME /commandhistory \
  && echo "$SNIPPET" >> "/home/$USERNAME/.bashrc"

# Claude configuration files
RUN mkdir /home/$USERNAME/.claude && chown -R $USERNAME /home/$USERNAME/.claude
# Codex configuration files
RUN mkdir /home/$USERNAME/.codex && chown -R $USERNAME /home/$USERNAME/.codex
# Rust toolchain
RUN mkdir /home/$USERNAME/.rustup && chown -R $USERNAME /home/$USERNAME/.rustup

# The devcontainer may contain credentials so this is not entirely safe. Use with caution.
RUN echo "alias claude-yolo='claude --dangerously-skip-permissions'" >> /etc/bash.bashrc
RUN echo "alias codex-yolo='codex --dangerously-bypass-approvals-and-sandbox'" >> /etc/bash.bashrc
