default:
    @just --list --justfile '{{justfile()}}'

install +TARGETS='--bin imgdup --bin imgdup-debug --bin imgdup-edit':
    cargo install --path . --locked {{TARGETS}}

@run BIN *ARGS:
    cargo run --bin {{BIN}} -- {{ARGS}}
