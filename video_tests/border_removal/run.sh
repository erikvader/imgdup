#!/bin/bash

set -e

# This test just runs border removal on all pics and outputs the results and intermediate
# steps for debugging.

EXE=../../target/release/border_remover
cargo build --bin border_remover --release

OUTPUT=output
MASKS=masks
rm -rf "$MASKS" "$OUTPUT"
mkdir -p "$MASKS" "$OUTPUT"

for pic in pics/*.jpg; do
    fname=$(basename "$pic" .jpg)
    echo "file: $fname"
    "$EXE" -o "$MASKS/$fname.png" --maskify "$@" "$pic" >/dev/null
    "$EXE" -o "$OUTPUT/$fname.jpg" "$@" "$pic"
done
