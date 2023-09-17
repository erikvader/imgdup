#!/bin/bash

rm -f imgdup.db

# Make sure that the very first frame is ignored and is placed in the graveyard. The
# resolution on the images in the ignore folder matters.
cargo run --bin imgdup
