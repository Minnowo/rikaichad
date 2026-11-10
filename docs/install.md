# Installing jisho

jisho is one binary with no runtime dependencies. There are no prebuilt
binaries or packages yet; build it from source.

## Requirements

- Rust 1.99 or newer (https://rustup.rs).
- A C compiler: SQLite and zstd are compiled from source as part of the
  build.
  - Linux: `gcc` or `clang` (Debian/Ubuntu: `sudo apt install
    build-essential`).
  - macOS: the Xcode command line tools (`xcode-select --install`).
  - Windows: the MSVC build tools that rustup offers to install.
- About 1 GB of disk for the build.

## Build and install

From a checkout of the repository:

```sh
cargo build --release
```

The binary is `target/release/jisho` (`jisho.exe` on Windows). Copy it to
a directory on your PATH, e.g. `~/.local/bin` on Linux and macOS.

Or let cargo build and install it into `~/.cargo/bin` in one step:

```sh
cargo install --path crates/jisho
```

The build records the git commit it was built from (shown by `GET
/v1/source`). Outside a git checkout, set `JISHO_GIT_COMMIT` to the
commit (and `JISHO_GIT_DIRTY=false` if unmodified) when building.

## Dictionaries

jisho reads Yomitan dictionary zips (format 3), the same files you import
into Yomitan. Free ones to start with: JMdict and KANJIDIC from
https://github.com/yomidevs/jmdict-yomitan/releases.

```sh
jisho import JMdict_english.zip KANJIDIC_english.zip
```

Importing a large dictionary takes seconds to a minute; 99 dictionaries
take about 15 minutes. Images and audio inside dictionaries are skipped.

## The store

All imported dictionaries live in one SQLite file, the store:

| Platform | Default location                                      |
|----------|-------------------------------------------------------|
| Linux    | `$XDG_DATA_HOME/jisho/jisho.sqlite` or `~/.local/share/jisho/jisho.sqlite` |
| macOS    | `~/Library/Application Support/jisho/jisho.sqlite`    |
| Windows  | `%APPDATA%\jisho\jisho.sqlite`                        |

`--store PATH` or the `JISHO_STORE` environment variable choose another
file. The store is self-contained: copy it (while jisho is not writing to
it) to use the same dictionaries on another machine, any platform. It also
keeps the dictionary order and settings (`jisho order`, `jisho
dictionary`, `jisho settings`).

Keep your dictionary zips. When a new jisho version changes the store
format, it refuses the old store with a message; delete the store file
and import the zips again.

## Running the server

```sh
jisho serve                                # 127.0.0.1:47613, this machine only
jisho serve --bind 0.0.0.0:47613 --read-only
```

Open http://127.0.0.1:47613/ in a browser for the web UI (lookup, kanji,
sentence parsing); it is built into the binary.

There is no authentication. On the default loopback address only programs
on this machine can connect; other web pages in your browser can call it
only from origins you allow with `--allow-origin` (the built-in web UI is
always allowed), and requests with a foreign `Host` header are refused
(DNS rebinding). With `--bind` on a network
address, anyone who can reach it can use it: add `--read-only` so nobody
can import, remove or change dictionaries (jisho warns when you do not).
To open the web UI by a host name rather than an IP address (e.g.
`http://jisho.lan:47613/`), add the name with `--allow-host`; behind a
reverse proxy, the proxy must pass the original `Host` header.
docs/api.md has the details and the limits.

To keep it running on Linux, a systemd user service works, for example
`~/.config/systemd/user/jisho.service`:

```ini
[Unit]
Description=jisho dictionary API

[Service]
ExecStart=%h/.local/bin/jisho serve --read-only
Restart=on-failure

[Install]
WantedBy=default.target
```

then `systemctl --user enable --now jisho`. Ctrl-C (or stopping the
service) cancels a running import; nothing half-imported is kept.

## Distributing binaries

A binary carries code under several licenses. Ship it with LICENSE,
NOTICE and the third-party licenses of the crates compiled in (including
the bundled zstd C library), generated from Cargo.lock:

```sh
python3 -I scripts/third-party-licenses.py > THIRD-PARTY-LICENSES.txt
```

As AGPL-3.0 and GPL-3.0 software, it must also come with its source (or
an offer of it) for the exact version built.

## Uninstalling

Delete the binary and the store file (see the table above; also delete
the `-wal` and `-shm` files next to it if present).
