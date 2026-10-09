FROM rust:1.99.0-bookworm AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY migrations ./migrations
COPY openapi.json ./
RUN cargo build --release --locked

FROM debian:bookworm-slim
COPY --from=build /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
WORKDIR /app
COPY --from=build /app/target/release/suasobra-backend /usr/local/bin/suasobra-backend
USER 65532:65532
ENV LISTEN_ADDR=0.0.0.0:8000
EXPOSE 8000
ENTRYPOINT ["suasobra-backend"]
CMD ["serve"]
