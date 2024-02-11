# FFmpeg
The `FrameExtractor` uses [FFmpeg](https://ffmpeg.org/), and there are
several options for linking against it. The rust bindings are from the
package [`ffmpeg-next`](https://crates.io/crates/ffmpeg-next)
([`ffmpeg-next-sys`](https://crates.io/crates/ffmpeg-sys-next)),
and the versions of it and FFmpeg must match.

## System installed
If the correct version of FFmpeg already is installed globally on the
sytem, then no special config is needed, just compile and run.

## Non-default system installed
It's possible to change what FFmpeg to use with environment variables,
set with e.g. `$IMGDUP/.cargo/config.toml`. The most straight forward
is to set the variable read by the build script of `ffmpeg-next-sys`:

```toml
[env]
FFMPEG_DIR = "/home/erik/Documents/imgdup/ffmpeg/install"
```

Another way is to help [pkgconf](http://pkgconf.org/), which the build
script uses, like:

```toml
[env]
PKG_CONFIG_PATH = "/usr/lib/ffmpeg4.4/pkgconfig"
```

## Compile and link statically
It's possible to tell the build script to download, compile and statically link FFmpeg with:

```toml
[env]
CARGO_FEATURE_STATIC = "yes"
CARGO_FEATURE_BUILD = "yes"
```

## Compile and link dynamically _(deprecated)_
Another option is to compile FFmpeg normally and install it locally in
this repo (or somewhere else) and point to it somehow when executing
the binary. There is a [justfile](https://github.com/casey/just)
prepared to build ffmpeg locally, invoke as `just all` in this
directory and set `FFMPEG_DIR` in the `config.toml` so the rust crate
can find it.

### Wrapper script
One option is to create a wrapper script that sets `LD_LIBRARY_PATH`
to the `lib` dir. Something like:

```sh
export LD_LIBRARY_PATH=$LD_LIBRARY_PATH:asd/lib
exec imgdup "$@"
```

This is implemented in the top-level justfile and is run with
`just install-wrapper`. This assumes that the binaries are installed
in this repo under the directory `install`, which can be set via the
`config.toml`:

```toml
[install]
root = "/home/erik/Documents/imgdup/install"
```

They shouldn't be on the `PATH`, at least, to avoid clashes.
The `LD_LIBRARY_PATH` is set to `$IMGDUP/ffmpeg/install/lib`, so the
custom compiled FFmpeg is assumed. The files can later be uninstalled
with `just uninstall-wrapper`.

### rpath
Another option is to embed the library path in the executable as an
[rpath](https://aimlesslygoingforward.com/blog/2014/01/19/bundling-shared-libraries-on-linux/)
(or actually
[runpath](https://amir.rachum.com/shared-libraries/#rpath-and-runpath)).
This is maybe [discouraged](https://wiki.debian.org/RpathIssue) as
only executables and shared libraries that actually has this value set
will be able to find the FFmpeg files. Thus, FFmpeg itself also needs
the rpath set, which the justfile adds by default.

To add it on the rust executable so it can find the shared objects:

```toml
[build]
rustflags = ["-C", "link-arg=-Wl,-rpath,/home/erik/Documents/imgdup/ffmpeg/install/lib"]
```

Also build FFmpeg with the variable `rpath=true` in the justfile to
add the rpath to FFmpeg shared objects as well.

Double check that it worked by running `readelf -d imgdup`, there
should be a line with: `Library runpath:
[/home/erik/Documents/imgdup/ffmpeg/install/lib]`. Then check that
`ldd imgdup` points to the correct files, in case it doesn't, it's
possible to debug with `LD_DEBUG=libs imgdup --help`
