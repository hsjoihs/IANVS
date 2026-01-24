FROM mcr.microsoft.com/devcontainers/base:2-bookworm@sha256:23fa69fed758b7927c60061317d73baf7d66b9fca5c344e80bce3a940b229af0

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

# Used to persist bash history. For details see: https://code.visualstudio.com/remote/advancedcontainers/persist-bash-history
RUN SNIPPET="export PROMPT_COMMAND='history -a' && export HISTFILE=/commandhistory/.bash_history" \
    && mkdir /commandhistory \
    && touch /commandhistory/.bash_history \
    && chown -R $USERNAME /commandhistory \
    && echo "$SNIPPET" >> "/home/$USERNAME/.bashrc"

# Claude configuration files
RUN mkdir /home/$USERNAME/.claude && chown -R $USERNAME /home/$USERNAME/.claude
# Rust toolchain
RUN mkdir /home/$USERNAME/.rustup && chown -R $USERNAME /home/$USERNAME/.rustup

# The devcontainer may contain Claude credentials so this is not entirely safe.
# Use with caution.
RUN echo "alias claude-yolo='claude --dangerously-skip-permissions'" >> /etc/bash.bashrc
