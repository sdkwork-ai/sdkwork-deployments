FROM rust:1-bookworm AS builder
COPY . /src
WORKDIR /src/sdkwork-deployments
RUN cargo build --release \
    -p sdkwork-api-deployments-standalone-gateway \
    -p sdkwork-deploy-runtime-assignment-worker \
    -p sdkwork-deploy-certificate-renewal-worker \
    -p sdkwork-deploy-certificate-worker

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && groupadd --gid 10001 sdkwork \
    && useradd --uid 10001 --gid 10001 --no-create-home --shell /usr/sbin/nologin sdkwork \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder --chown=10001:10001 /src/sdkwork-deployments/target/release/sdkwork-api-deployments-standalone-gateway /usr/local/bin/sdkwork-api-deployments-standalone-gateway
COPY --from=builder --chown=10001:10001 /src/sdkwork-deployments/target/release/sdkwork-deploy-runtime-assignment-worker /usr/local/bin/sdkwork-deploy-runtime-assignment-worker
COPY --from=builder --chown=10001:10001 /src/sdkwork-deployments/target/release/sdkwork-deploy-certificate-renewal-worker /usr/local/bin/sdkwork-deploy-certificate-renewal-worker
COPY --from=builder --chown=10001:10001 /src/sdkwork-deployments/target/release/sdkwork-deploy-certificate-worker /usr/local/bin/sdkwork-deploy-certificate-worker
COPY --from=builder --chown=10001:10001 /src/sdkwork-deployments/database /app/database
ENV SDKWORK_DEPLOY_APP_ROOT=/app
ENV SDKWORK_DEPLOY_ENVIRONMENT=production
ENV SDKWORK_ENVIRONMENT=production
ENV SDKWORK_DEPLOY_DEPLOYMENT_PROFILE=cloud
ENV SDKWORK_DEPLOY_PROFILE_ID=cloud.production
# Spec (ENVIRONMENT_SPEC §server-bind / NGINX_SPEC): the standalone gateway listens
# on 3900 inside the container. 8080 is explicitly obsolete and MUST NOT be used.
ENV SDKWORK_DEPLOY_APPLICATION_PUBLIC_INGRESS_BIND=0.0.0.0:3900
USER 10001:10001
EXPOSE 3900
CMD ["sdkwork-api-deployments-standalone-gateway"]
