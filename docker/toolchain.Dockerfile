# syntax=docker/dockerfile:1
#
# The toolchain image, ghcr.io/slavkobojanic/kobra:<version> (docs/design/38-deployment.md,
# "The toolchain image"): the released `k` and CA certificates, for the build
# stage of an app's Dockerfile. The context is a directory holding the `k` of
# one Linux release archive; the release and nightly workflows build it:
#
#   docker build --file docker/toolchain.Dockerfile --tag k-toolchain:ci target/image
#
# The base is debian-slim rather than distroless: the image is a build
# stage, and app Dockerfiles (examples/shop/Dockerfile, the book's samples)
# use shell-form `RUN k check`, which needs /bin/sh. The app's runtime
# image stays distroless/cc. `k` is built against glibc 2.31, so it runs
# here.
FROM debian:bookworm-slim
LABEL org.opencontainers.image.title="Kobra toolchain" \
      org.opencontainers.image.description="The k command of the Kobra language: compiler, test runner and server runtime" \
      org.opencontainers.image.source="https://github.com/slavkobojanic/kobra" \
      org.opencontainers.image.licenses="MIT OR Apache-2.0"
RUN apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY k /usr/local/bin/k
RUN k --version
WORKDIR /src
ENTRYPOINT ["k"]
