default:
    @just --list --justfile '{{justfile()}}'

bin_dir := env('HOME') / '.bin'
install_dir := justfile_directory() / 'install'
runner := 'imgdup-runner.sh'
targets := 'imgdup imgdup-debug imgdup-edit'

install:
    #TODO: find a better way to prepend stuff to a list
    cargo install --path . --locked $(printf -- '--bin %s ' {{targets}})

install-wrapper: install
    install '{{runner}}' '{{install_dir}}'
    sed -i 's|ROOT=$PWD|ROOT="{{justfile_directory()}}"|' '{{install_dir / runner}}'
    for t in {{targets}}; do ln -sf '{{install_dir / runner}}' '{{bin_dir}}/'"$t"; done

uninstall-wrapper:
    #TODO: find a better way to prepend stuff to a list
    rm -f '{{install_dir / runner}}' $(printf -- '{{bin_dir}}/%s ' {{targets}})

set positional-arguments

@run MODE BIN *ARGS:
    cargo build {{ if MODE == "release" {"--release"} else {""} }} --bin {{BIN}} >/dev/null 2>&1
    BINS='{{justfile_directory()}}/target/{{MODE}}' SELF={{BIN}} sh '{{justfile_directory() / runner}}' "${@:3}"
