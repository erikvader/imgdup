#!/bin/bash

# Compares images to get a feel for hash algorithm performance

cargo run --release --bin pic_comparator -- -c 34 frames frames2
