#!/bin/bash

set -e

OUTDIR=frames
rm -rf "$OUTDIR"
mkdir -p "$OUTDIR"
VIDEOS=$1

cargo build --bin random_frame --release
EXE=../../target/release/random_frame

while IFS='' read -r line || [[ -n "$line" ]]; do
    fname=${line##*/}
    fname=${fname%.*}
    echo "$fname"
    outpath="$OUTDIR/$fname.jpg"
    if [[ -e $outpath ]]; then
        echo this already exists
        exit 1
    fi
    "$EXE" "$line" "$outpath"
    if [[ ! -e $outpath ]]; then
        echo did not write anything
        exit 1
    fi
done < <(find "$VIDEOS" -mindepth 1 -maxdepth 1 -type f)
