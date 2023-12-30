#!/bin/sh

# A wrapper that adds directories to LD_LIBRARY_PATH to enable locally built shared
# libraries.

ROOT=$PWD
FFMPEG=${FFMPEG:-$ROOT/ffmpeg/install/lib}
BINS=${BINS:-$ROOT/install/bin}

SELF=${SELF:-${0##*/}}

export LD_LIBRARY_PATH="$FFMPEG${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"

exec "$BINS/$SELF" "$@"
