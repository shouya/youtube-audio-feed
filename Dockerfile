FROM docker.io/library/rust:1-alpine AS BUILDER

COPY . /workdir
WORKDIR /workdir

RUN cargo build --release --target=x86_64-unknown-linux-musl

FROM docker.io/library/alpine:latest AS RUNNER

COPY --from=BUILDER \
    /workdir/target/release/youtube_audio_feed \
    /usr/bin/youtube_audio_feed

EXPOSE 8080
CMD /usr/bin/youtube_audio_feed
