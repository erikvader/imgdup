set positional-arguments
set ignore-comments

default:
    @just --list --justfile '{{justfile()}}'

bin_dir := env('HOME') / '.bin'
install_dir := justfile_directory() / 'install'
runner := 'imgdup-runner.sh'
videotargets := 'videodup videodup-debug videodup-edit'
targets := videotargets

# Install the rust binaries
install:
    #TODO: find a better way to prepend stuff to a list
    cargo install --path videodup --locked $(printf -- '--bin %s ' {{videotargets}})

# Install the wrappers and rust binaries
install-wrapper: install
    install '{{runner}}' '{{install_dir}}'
    sed -i 's|ROOT=$PWD|ROOT="{{justfile_directory()}}"|' '{{install_dir / runner}}'
    for t in {{targets}}; do ln -vsf '{{install_dir / runner}}' '{{bin_dir}}/'"$t"; done

# Uninstall the wrappers
uninstall-wrapper:
    #TODO: find a better way to prepend stuff to a list
    rm -vrf '{{install_dir}}' $(printf -- '{{bin_dir}}/%s ' {{targets}})

# Builds and runs a bin using the wrapper to make sure the correct shared libraries are
# used.
run MODE BIN *ARGS:
    cargo build {{ if MODE == "release" {"--release"} else {""} }} --bin {{BIN}} >/dev/null 2>&1
    BINS='{{justfile_directory()}}/target/{{MODE}}' SELF={{BIN}} sh '{{justfile_directory() / runner}}' "${@:3}"
