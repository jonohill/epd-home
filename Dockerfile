FROM --platform=$BUILDPLATFORM rust:1.76.0 AS build

ARG TARGETPLATFORM
ARG BUILDPLATFORM

RUN apt-get update && apt-get install -y \
        jq \
    && rm -rf /var/lib/apt/lists/*

RUN url="$(curl -sL https://ziglang.org/download/index.json | jq -r --arg arch "$(uname -m)-linux" '.master[$arch].tarball')" && \
    mkdir -p /opt/zig && \
    curl -sL "$url" | tar -xJf - -C /opt/zig --strip-components=1
ENV PATH="/opt/zig:$PATH"

RUN if [ "$TARGETPLATFORM" = "linux/amd64" ]; then \
        echo "x86_64-unknown-linux-musl" > /target; \
    elif [ "$TARGETPLATFORM" = "linux/arm64" ]; then \
        echo "aarch64-unknown-linux-musl" > /target; \
    fi

RUN rustup target add "$(cat /target)" && \
    rustup component add rustfmt

RUN cargo install cargo-zigbuild

WORKDIR /src
COPY . .

RUN cd epd-home-web && \
    cargo zigbuild --target "$(cat /target)" --release && \
    cp "../target/$(cat /target)/release/epd-home-web" /epd-home-web

FROM rust:1.76.0 AS build-native

COPY --from=0 /epd-home-web /epd-home-web
RUN strip /epd-home-web

FROM alpine:3.19

ARG TARGETPLATFORM

RUN addgroup -S app && adduser -S app -G app
RUN mkdir -p /data && chown -R app:app /data

COPY --from=build-native /epd-home-web /epd-home-web
RUN chmod +x /epd-home-web

USER app

ENV PORT="8080"
ENV LISTEN_ADDRESS="0.0.0.0:${PORT}"

ENTRYPOINT [ "/epd-home-web" ]
