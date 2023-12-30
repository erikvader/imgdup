#!/bin/sh

ROOT="CHANGE-ME-PLEASE"
FFMPEG="$ROOT/ffmpeg/install/lib"
BINS="$ROOT/install/bin"

SELF=${0##*/}

export LD_LIBRARY_PATH="$FFMPEG${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"

exec "$BINS/$SELF" "$@"
