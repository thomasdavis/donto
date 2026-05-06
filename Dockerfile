FROM rust:1.86-slim-bookworm AS build
WORKDIR /app
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
COPY . .
RUN cargo build --release -p dontosrv

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*
COPY --from=build /app/target/release/dontosrv /usr/local/bin/dontosrv
ENV DONTO_BIND=0.0.0.0:7878
EXPOSE 7878
CMD ["dontosrv"]
