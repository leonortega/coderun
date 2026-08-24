# v0.4.0 multi-stage: rust builder → distroless runtime
FROM rust:1.75-slim AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo build --release

FROM gcr.io/distroless/cc-debian12
COPY --from=builder /app/target/release/coderun /usr/local/bin/coderun
COPY --from=builder /app/target/release/coderun-daemon /usr/local/bin/coderun-daemon
EXPOSE 9527 3001 9090
ENTRYPOINT ["coderun-daemon"]
