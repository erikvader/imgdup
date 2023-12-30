default:
    @just --list --justfile '{{justfile()}}'

bin_dir := env('HOME') / '.bin'
install_dir := justfile_directory() / 'install'
runner := 'imgdup-runner.sh'
targets := 'imgdup imgdup-debug imgdup-edit'

install:
    cargo install --path . --locked $(printf -- '--bin %s ' {{targets}})

install-wrapper: install
    install '{{runner}}' '{{install_dir}}'
    sed -i 's|ROOT="CHANGE-ME-PLEASE"|ROOT="{{justfile_directory()}}"|' '{{install_dir / runner}}'
    for t in {{targets}}; do ln -sf '{{install_dir / runner}}' '{{bin_dir}}/'"$t"; done

uninstall-wrapper:
    #TODO: find a better way to prepend stuff to a list
    rm -f '{{install_dir / runner}}' $(printf -- '{{bin_dir}}/%s ' {{targets}})

@run BIN *ARGS:
    cargo run --bin {{BIN}} -- {{ARGS}}
