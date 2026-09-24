FROM rust:1.98.1-slim-bookworm@sha256:ff521445a372125ed4f76e1453a1f8098f2d05332d1601d30db1c1f62757e730 AS build
ARG TARGETARCH
RUN test "$TARGETARCH" = arm64 || { echo 'Only linux/arm64 is validated and supported by this Dockerfile' >&2; exit 1; }
RUN apt-get update && apt-get install -y --no-install-recommends \
    g++ libc6-dev pkg-config ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
RUN curl -fL --retry 3 \
    https://github.com/microsoft/onnxruntime/releases/download/v1.28.0/onnxruntime-linux-aarch64-1.28.0.tgz \
    -o /tmp/ort.tgz \
    && echo 'e15ff8b5d85afe6c144d97c6fd432254bf76a219daaf17658087d6ecb3e8f0bb  /tmp/ort.tgz' | sha256sum -c - \
    && mkdir /ort && tar -xzf /tmp/ort.tgz -C /ort --strip-components=1 \
    && rm /tmp/ort.tgz && rm -rf /ort/lib/cmake /ort/lib/pkgconfig
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY docs/model-manifest.json ./docs/model-manifest.json
RUN cargo build --locked --release --bin laya-server && strip target/release/laya-server
# Retain crate/native subcomponent notices and the linked Rust standard library notices.
RUN mkdir -p /licenses/crates /licenses/rust \
    && cd /usr/local/cargo/registry/src && find . -type f \
    \( -iname '*license*' -o -iname '*copying*' -o -iname '*copyright*' \
    -o -iname '*notice*' -o -name Cargo.toml \) -exec cp --parents -t /licenses/crates {} + \
    && cp -r /usr/local/rustup/toolchains/*/share/doc/rust/licenses /licenses/rust/ \
    && cp /usr/local/rustup/toolchains/*/share/doc/rust/COPYRIGHT*.html /licenses/rust/

FROM debian:bookworm-slim@sha256:3783cc01769c7b2b1b83a5c5ad96c815348e28ed7da68e2e3687004faa906251
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libstdc++6 libgcc-s1 \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /build/target/release/laya-server /usr/local/bin/laya-server
COPY --from=build /ort/lib/ /opt/onnxruntime/lib/
COPY --from=build /ort/LICENSE /ort/ThirdPartyNotices.txt /usr/share/doc/onnxruntime/
COPY --from=build /licenses/ /usr/share/doc/laya-server/dependencies/
COPY LICENSE NOTICE.md Cargo.lock /usr/share/doc/laya-server/
COPY licenses/ /usr/share/doc/laya-server/licenses/
ENV LD_LIBRARY_PATH=/opt/onnxruntime/lib
USER 65532:65532
EXPOSE 8080
STOPSIGNAL SIGTERM
ENTRYPOINT ["/usr/local/bin/laya-server"]
CMD ["--model", "/models/multilingual", "--threads", "4", "--max-concurrency", "1"]
