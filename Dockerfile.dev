# syntax=docker/dockerfile:1.7
# Dev container for sinfonia: Node.js + Rust toolchain + Claude Code + gh,
# intended to run with --dangerously-skip-permissions against a bind-mounted repo.

FROM node:22-bookworm-slim

ARG USER_UID=501
ARG USER_GID=20
ARG USERNAME=dev

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y --no-install-recommends \
      git \
      curl \
      ca-certificates \
      gnupg \
      build-essential \
      pkg-config \
      libssl-dev \
      ripgrep \
      jq \
      less \
      sudo \
      openssh-client \
    && curl -fsSL https://cli.github.com/packages/githubcli-archive-keyring.gpg \
         | dd of=/usr/share/keyrings/githubcli-archive-keyring.gpg \
    && chmod go+r /usr/share/keyrings/githubcli-archive-keyring.gpg \
    && echo "deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/githubcli-archive-keyring.gpg] https://cli.github.com/packages stable main" \
         > /etc/apt/sources.list.d/github-cli.list \
    && apt-get update && apt-get install -y --no-install-recommends gh \
    && rm -rf /var/lib/apt/lists/*

RUN npm install -g @anthropic-ai/claude-code

RUN if ! getent group ${USER_GID} >/dev/null; then groupadd -g ${USER_GID} ${USERNAME}; fi \
 && useradd -m -u ${USER_UID} -g ${USER_GID} -s /bin/bash ${USERNAME} \
 && echo "${USERNAME} ALL=(ALL) NOPASSWD:ALL" > /etc/sudoers.d/${USERNAME}

COPY docker/entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh

USER ${USERNAME}
WORKDIR /workspace

ENV RUSTUP_HOME=/home/${USERNAME}/.rustup \
    CARGO_HOME=/home/${USERNAME}/.cargo \
    PATH=/home/${USERNAME}/.cargo/bin:/usr/local/bin:/usr/bin:/bin
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --default-toolchain stable --profile minimal \
 && rustup component add rustfmt clippy

ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
CMD ["claude", "--dangerously-skip-permissions"]
