#!/bin/bash

set -e

VIDEOVERTICAL='vertical-5hImWP9_h_0.mkv'
VIDEOFAULTY='faulty_[gsmisy].mp4'
VIDEOANALYZE='more_analyze_ganme2_000.mp4'
VIDEOINVALID='invalid_data.wmv'

rm -rf frames_vertical frames_faulty frames_analyze frames_invalid

# - The width is not a multiple of 16, so there is padding in the RGB buffer
# - The video stream's duration is AV_NOPTS_VALUE
cargo run --bin frame_extractor -- --outdir frames_vertical --num 1 "$VIDEOVERTICAL"

# There are missing frames at around 00:10:40
cargo run --bin frame_extractor -- --offset '10min 43s' --step 0s --outdir frames_faulty --num 30 "$VIDEOFAULTY"

# This video needed a larger analyzeduration to find streams info
cargo run --bin frame_extractor -- --outdir frames_analyze --num 1 "$VIDEOANALYZE"

# This video has invalid data at around 00:06:10, which needs to be skipped
cargo run --bin frame_extractor -- --step 1s --offset '6min 10s' --outdir frames_invalid --num 20 "$VIDEOINVALID"
