# Headless Octa image: runs the CLI actions and the `--mcp` stdio server.
# The GUI is intentionally not usable here - eframe/rfd load the windowing
# libraries (GTK/X11/Wayland) lazily at runtime, and the headless paths
# (--mcp and the CLI flags) never touch them, so they are dropped entirely.
#
# Two stages:
#   * builder  - full Rust toolchain + the GUI dev headers the crate needs to
#                *compile* (gtk-sys / rfd / x11 bindings resolve at build time
#                even though they are never dlopen'd in headless mode).
#   * runtime  - gcr.io/distroless/cc-debian12: glibc + libstdc++ + libgcc only.
#
# Podman builds and runs this unchanged: `podman build` / `podman run`.

# ---- builder ----------------------------------------------------------------
FROM rust:1-bookworm AS builder

# build-essential: g++/make for the bundled DuckDB and rusqlite C/C++ sources.
# The libgtk/xcb/xkb/fontconfig/freetype/ssl -dev packages satisfy the GUI
# crates' build scripts; they do not end up in the runtime image.
RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential \
        cmake \
        pkg-config \
        libgtk-3-dev \
        libxcb-render0-dev \
        libxcb-shape0-dev \
        libxcb-xfixes0-dev \
        libxkbcommon-dev \
        libssl-dev \
        libfontconfig1-dev \
        libfreetype6-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Copy only what the build actually consumes - never `COPY . .`. The Rust build
# needs the manifests, build.rs, and the source tree; the crate also embeds the
# fonts/SVGs/syntax under assets/ and the i18n catalogs under locales/ via
# include_bytes!/include_str!. LICENSE, THIRD_PARTY_LICENSES.md, and licenses/
# are not used by the compile - they ride along only so the runtime stage can
# pull them from this stage. Adding an embedded asset or a new top-level input
# means adding it here too.
COPY Cargo.toml Cargo.lock build.rs ./
COPY src ./src
COPY assets ./assets
COPY locales ./locales
COPY LICENSE THIRD_PARTY_LICENSES.md ./
COPY licenses ./licenses

# Release builds pass --build-arg OCTA_VERSION=X.Y.Z to stamp the binary's
# embedded CARGO_PKG_VERSION (octa --version / About dialog). Local builds omit
# it and stay on the locked 0.0.0-dev placeholder. Sed'ing the version makes
# Cargo.lock's octa entry stale, so --locked is only used in the unstamped
# (local) path - the release path mirrors release.yml's plain build.
ARG OCTA_VERSION=
RUN if [ -n "$OCTA_VERSION" ]; then \
        sed -i "s/^version = .*/version = \"$OCTA_VERSION\"/" Cargo.toml && \
        cargo build --release ; \
    else \
        cargo build --release --locked ; \
    fi

# Stage the entire runtime payload under /out, mirroring the destination layout,
# so the runtime stage needs exactly one COPY. liblzma is the only NEEDED
# library beyond what distroless/cc ships (glibc, libstdc++, libgcc, libm);
# cp resolves the versioned symlink to a real file. The licenses live under
# /usr/share/octa, mirroring what install.sh ships alongside the binary.
#
# distroless has no shell/useradd, so we hand-write a minimal passwd/group
# defining a non-root `octa` user (uid 65532, the conventional distroless
# nonroot id) plus root, and give it a home dir. These land under /out/etc and
# get merged over distroless's own passwd/group by the single COPY below
# (other /etc files like the CA bundle are left untouched).
RUN set -eux; \
    mkdir -p /out/usr/local/bin /out/usr/share/octa /out/usr/lib/x86_64-linux-gnu \
             /out/etc /out/home/octa; \
    cp target/release/octa            /out/usr/local/bin/octa; \
    cp THIRD_PARTY_LICENSES.md LICENSE /out/usr/share/octa/; \
    cp -r licenses                    /out/usr/share/octa/licenses; \
    cp /usr/lib/x86_64-linux-gnu/liblzma.so.5 /out/usr/lib/x86_64-linux-gnu/liblzma.so.5; \
    printf 'root:x:0:0:root:/root:/usr/sbin/nologin\nocta:x:65532:65532:octa:/home/octa:/usr/sbin/nologin\n' > /out/etc/passwd; \
    printf 'root:x:0:\nocta:x:65532:\n' > /out/etc/group; \
    chown -R 65532:65532 /out/home/octa

# ---- runtime ----------------------------------------------------------------
FROM gcr.io/distroless/cc-debian12

# Single copy of the pre-assembled tree (binary + liblzma + licenses + the
# octa user's passwd/group/home). COPY --from preserves the builder's uid/gid,
# so /home/octa stays owned by 65532.
COPY --from=builder /out/ /

# Run as the non-root `octa` user so the container never executes as root.
# HOME points at the owned home dir so config lookups (~/.config/octa) resolve.
USER octa:octa
ENV HOME=/home/octa

# No default action flag: `docker run octa --mcp` starts the MCP server,
# `docker run octa --schema /data/file.parquet` runs a one-shot CLI action.
ENTRYPOINT ["/usr/local/bin/octa"]
