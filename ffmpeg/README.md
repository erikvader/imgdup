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

## Compile and link dynamically
Another option is to compile FFmpeg normally and install it locally in
this repo (or somewhere else). There is a
[justfile](https://github.com/casey/just) prepared to do just that,
invoke as `just all`. Then, add the lib path as an
[rpath](https://aimlesslygoingforward.com/blog/2014/01/19/bundling-shared-libraries-on-linux/)
on the executable so it can find the shared objects:

```toml
[build]
rustflags = ["-C", "link-arg=-Wl,-rpath,/home/erik/Documents/imgdup/ffmpeg/install/lib"]
```

Double check that it worked by running `readelf -d imgdup`, there
should be a line with: `Library runpath:
[/home/erik/Documents/imgdup/ffmpeg/install/lib]`. Then check that
`ldd imgdup` points to the correct files.
