#!/bin/sh

# https://www.youtube.com/watch?v=pHvP71rwYAc
VIDEONORMAL='10 Minute Timer (count-up stopwatch) [pHvP71rwYAc].webm'
# The first five seconds have been removed but with timestamps preserved, so the first
# timestamp in the video starts at 5, not 0.
VIDEOSHIFTED='10 Minute Timer (count-up stopwatch) [pHvP71rwYAc] shifted.webm'

rm -rf frames_normal frames_shifted
mkdir frames_shifted
mkdir frames_normal

# 10 frames with the timer in the images counting up from 0
cargo run --bin frame_extractor "$VIDEONORMAL" 1 frames_normal 10
# 10 frames with the timer in the images counting up from 5, but the timestamp in the
# filename starts at 0.
cargo run --bin frame_extractor "$VIDEOSHIFTED" 1 frames_shifted 10
