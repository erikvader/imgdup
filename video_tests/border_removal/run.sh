#!/bin/bash

set -e

EXE=../../target/release/border_remover
cargo build --bin border_remover --release

OUTPUT=output
MASKS=masks
rm -rf "$MASKS" "$OUTPUT"
mkdir -p "$MASKS" "$OUTPUT"

threshold=${1:-20}
maxwhites=${2:-0.03}

for pic in pics/*.jpg; do
    fname=$(basename "$pic" .jpg)
    echo "file: $fname"
    "$EXE" -o "$MASKS/$fname.jpg" --maskify -t "$threshold" "$pic" >/dev/null
    "$EXE" -o "$OUTPUT/$fname.jpg" -t "$threshold" -w "$maxwhites" "$pic"
done
