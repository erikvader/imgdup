#!/bin/bash

set -e

mkdir -p _GRAVEYARD_ _DUPS_

# Will do exactly the same thing as the real deal, but with fewer video files
cargo run --release --bin imgdup
