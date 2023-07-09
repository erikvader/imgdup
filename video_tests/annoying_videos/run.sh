#!/bin/bash

set -e

VIDEOVERTICAL='vertical-5hImWP9_h_0.mkv'
VIDEOFAULTY='faulty_[gsmisy].mp4'

rm -rf frames_vertical frames_faulty

# - The width is not a multiple of 16, so there is padding in the RGB buffer
# - The video stream's duration is AV_NOPTS_VALUE
cargo run --bin frame_extractor -- --outdir frames_vertical --num 1 "$VIDEOVERTICAL"

# There are missing frames at around 00:10:40
cargo run --bin frame_extractor -- --offset '10min 43s' --step 0s --outdir frames_faulty --num 30 "$VIDEOFAULTY"
