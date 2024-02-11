default:
    @just --list --justfile '{{justfile()}}'

videotargets := 'videodup videodup-debug videodup-edit'

# Install the rust binaries
install:
    #TODO: find a better way to prepend stuff to a list
    cargo install --path videodup --locked $(printf -- '--bin %s ' {{videotargets}})

