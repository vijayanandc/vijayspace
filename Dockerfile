# The `weft` server image: one binary for `serve`, `api`, `worker` and `migrate` (see docs/operations.md).
#   docker build -t weft .
#   docker run --env-file weft.env weft worker
FROM rust:1.97-slim-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p weft-server --bin weft \
 && strip target/release/weft

FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates tzdata \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --no-create-home --shell /usr/sbin/nologin weft
COPY --from=build /src/target/release/weft /usr/local/bin/weft
USER weft
# Listen on every interface inside the container; logs as one JSON object per line for log shipping.
ENV WEFT_BIND=0.0.0.0:8080 WEFT_LOG_FORMAT=json
EXPOSE 8080
ENTRYPOINT ["weft"]
CMD ["serve"]
