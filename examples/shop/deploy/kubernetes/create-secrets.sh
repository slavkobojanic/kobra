#!/usr/bin/env bash
#
# Creates (or updates) the Kubernetes Secrets from the files in secrets/,
# which scripts/init-secrets.sh generates and which are never committed. Each
# file becomes a key that the pods mount under /run/secrets, so the
# containers read the same paths as with Docker Compose.
#
# Usage: deploy/kubernetes/create-secrets.sh
# Needs: kubectl, pointed at the cluster (kind sets the context).

set -euo pipefail

cd "$(dirname "$0")/../.."

for name in db_password paybox_db_password admin_password_hash paybox_service_token shop_service_token; do
    [[ -s "secrets/$name.txt" ]] || { echo "create-secrets: secrets/$name.txt is missing; run scripts/init-secrets.sh" >&2; exit 2; }
done

# Prints a Secret's manifest without creating it, so `apply` can create or
# update it: secret <name> --from-file=<key>=<path>...
secret() {
    local name="$1"
    shift
    kubectl create secret generic "$name" --namespace k-shop --dry-run=client --output yaml "$@" \
        | kubectl apply --filename -
}

kubectl apply --filename deploy/kubernetes/namespace.yaml

# Postgres: the shop's role is the superuser; the init script reads paybox's.
secret postgres-secrets \
    --from-file=db_password=secrets/db_password.txt \
    --from-file=paybox_db_password=secrets/paybox_db_password.txt

# Paybox verifies the shop with paybox_service_token and signs its callbacks
# with shop_service_token.
secret paybox-secrets \
    --from-file=paybox_db_password=secrets/paybox_db_password.txt \
    --from-file=paybox_service_token=secrets/paybox_service_token.txt \
    --from-file=shop_service_token=secrets/shop_service_token.txt

# The shop verifies paybox with shop_service_token and signs its calls with
# paybox_service_token.
secret shop-secrets \
    --from-file=db_password=secrets/db_password.txt \
    --from-file=admin_password_hash=secrets/admin_password_hash.txt \
    --from-file=shop_service_token=secrets/shop_service_token.txt \
    --from-file=paybox_service_token=secrets/paybox_service_token.txt
