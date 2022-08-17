FROM docker.io/library/rust:1-alpine AS BUILDER

RUN apk add --no-cache musl-dev

COPY . /workdir
WORKDIR /workdir

RUN --mount=type=cache,target=/workdir/target \
    --mount=type=cache,target=/root/.cargo \
    cargo build --release --target=x86_64-unknown-linux-musl
# RUN cargo build --release --target=x86_64-unknown-linux-musl

FROM docker.io/library/alpine:latest AS RUNNER

COPY --from=BUILDER \
    /workdir/target/x86_64-unknown-linux-musl/release/youtube_audio_feed \
    /usr/bin/youtube_audio_feed

EXPOSE 8080
CMD /usr/bin/youtube_audio_feed
