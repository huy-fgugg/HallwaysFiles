# Build context is the repository root (the server shares src/net/proto.rs with the game).
FROM rust:1-slim AS build
WORKDIR /app
COPY server/Cargo.toml server/Cargo.lock* server/
COPY server/src server/src
COPY src/net/proto.rs src/net/proto.rs
RUN cd server && cargo build --release

FROM debian:bookworm-slim
COPY --from=build /app/server/target/release/hallways-server /usr/local/bin/hallways-server
ENV PORT=10000
EXPOSE 10000
CMD ["hallways-server"]
