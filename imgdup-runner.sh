#!/bin/sh

# A wrapper that adds directories to LD_LIBRARY_PATH to enable locally built shared
# libraries. Also makes sure the backtrace always is captured for easier debugging.

# TODO: just use some kind of container instead of messing with locally compiled libraries
# and search paths? This isn't perfect either since the dependencies of ffmpeg aren't
# taken care of...

ROOT=$PWD
FFMPEG=${FFMPEG:-$ROOT/ffmpeg/install/lib}
BINS=${BINS:-$ROOT/install/bin}

SELF=${SELF:-${0##*/}}

LD_LIBRARY_PATH=$FFMPEG${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}
export LD_LIBRARY_PATH

RUST_BACKTRACE=${RUST_BACKTRACE:-1}
export RUST_BACKTRACE

exec "$BINS/$SELF" "$@"
