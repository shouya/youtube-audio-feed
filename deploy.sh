#!/bin/bash

podman build -t youtube-audio-feed .
podman push --format v2s2 youtube-audio-feed \
       docker://registry.fly.io/youtube-audio-feed:latest

flyctl deploy
