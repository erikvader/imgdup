#!/bin/bash

set -e

# https://www.youtube.com/watch?v=pHvP71rwYAc
VIDEONORMAL='10 Minute Timer (count-up stopwatch) [pHvP71rwYAc].webm'
# The first five seconds have been removed but with timestamps preserved, so the first
# timestamp in the video starts at 5, not 0.
VIDEOSHIFTED='10 Minute Timer (count-up stopwatch) [pHvP71rwYAc] shifted.webm'

rm -rf frames_normal frames_shifted frames_last

# 10 frames with the timer in the images counting up from 0
cargo run --bin frame_extractor -- --step 1s --outdir frames_normal --num 10 "$VIDEONORMAL"
# 10 frames with the timer in the images counting up from 5, but the timestamp in the
# filename starts at 0.
cargo run --bin frame_extractor -- --step 1s --outdir frames_shifted --num 10 "$VIDEOSHIFTED"

# Make sure the very last frame can be extracted
cargo run --bin frame_extractor -- --step 0s --outdir frames_last --offset '9min 58s' --num 10000 "$VIDEONORMAL"
