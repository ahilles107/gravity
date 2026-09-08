# Optional runner image using a public base. Workflow actions install toolchains.
FROM myoung34/github-runner:ubuntu-noble
USER root
RUN apt-get update \
  && apt-get install -y --no-install-recommends build-essential pkg-config libssl-dev \
  && rm -rf /var/lib/apt/lists/*
