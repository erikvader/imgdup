#!/bin/bash

set -e

rm -fr imgdup.db _GRAVEYARD_ _DUPS_
mkdir -p _GRAVEYARD_ _DUPS_

# Make sure that the very first frame is ignored and is placed in the graveyard. The
# resolution on the images in the ignore folder matters.
cargo run --bin imgdup
