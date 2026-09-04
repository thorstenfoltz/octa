# A file with no extension at all. Octa matches it by name, opens it as text
# and colours it, and the sidebar's extension filter keeps it visible.
FROM rust:1-slim AS build
WORKDIR /src
COPY . .
RUN cargo build --release

FROM debian:stable-slim
COPY --from=build /src/target/release/octa /usr/local/bin/octa
ENTRYPOINT ["octa"]
