FROM docker.io/library/rust:1-alpine AS BUILDER

RUN apk add --no-cache musl-dev

COPY . /workdir
WORKDIR /workdir

RUN --mount=type=cache,target=/workdir/target \
    --mount=type=cache,target=/usr/local/cargo/registry \
    cargo build --release --target=x86_64-unknown-linux-musl && \
    cp /workdir/target/x86_64-unknown-linux-musl/release/youtube_audio_feed \
       /workdir


# RUN cargo build --release --target=x86_64-unknown-linux-musl

FROM docker.io/library/alpine:latest AS RUNNER

COPY --from=BUILDER \
    /workdir/youtube_audio_feed \
    /usr/bin/youtube_audio_feed

EXPOSE 8080
CMD /usr/bin/youtube_audio_feed
