#!/usr/bin/env bash
#
# Generates the secrets docker-compose.yml and the Kubernetes manifests
# read, as files in secrets/ (git-ignored), for a local run. A file that
# already exists is kept, so running it again changes nothing.
#
# Usage: scripts/init-secrets.sh [directory]
#   directory: where to write the files; secrets/ by default. scripts/e2e.sh
#   passes a temporary one.
# Needs: openssl; argon2, or docker to run it in a throwaway container.
#
# Every value is random. The admin password is printed once, and kept in
# admin_password.txt next to them so you can sign in at /sign-in as
# admin@shop.example; only its Argon2id hash is given to the shop.
#
# Two services, two service secrets (docs/design/40-workspaces-and-services.md):
# paybox verifies the shop's calls with paybox_service_token (the shop signs
# with it as PAYBOX_TOKEN), and the shop verifies paybox's callbacks with
# shop_service_token (paybox signs with it as PAYBOX_CALLBACK_TOKEN).

set -euo pipefail

cd "$(dirname "$0")/.."
readonly dir="${1:-secrets}"
mkdir -p "$dir"

# Writes <dir>/<name>.txt with <value>, unless it exists.
secret() {
    local file="$dir/$1.txt"
    if [[ -e "$file" ]]; then
        printf 'kept       %s\n' "$file"
        return
    fi
    printf '%s' "$2" > "$file"
    printf 'generated  %s\n' "$file"
}

random() {
    openssl rand -hex 24
}

# The PHC string of an Argon2id hash of $1 (64 MiB, 3 iterations, 4 lanes),
# the format AdminConfig requires.
argon2id() {
    local salt
    salt="$(openssl rand -hex 16)"
    if command -v argon2 >/dev/null; then
        printf '%s' "$1" | argon2 "$salt" -id -t 3 -m 16 -p 4 -e
    else
        docker pull --quiet alpine:3.22 >/dev/null
        printf '%s' "$1" | docker run --rm --interactive alpine:3.22 \
            sh -c "apk add --quiet --no-cache argon2 >/dev/null && argon2 '$salt' -id -t 3 -m 16 -p 4 -e"
    fi
}

command -v openssl >/dev/null || { echo "init-secrets: openssl is required" >&2; exit 2; }

secret db_password "$(random)"
secret paybox_db_password "$(random)"
secret paybox_service_token "$(random)"
secret shop_service_token "$(random)"

if [[ ! -e "$dir/admin_password_hash.txt" ]]; then
    password="$(openssl rand -hex 12)"
    printf '%s' "$password" > "$dir/admin_password.txt"
    secret admin_password_hash "$(argon2id "$password")"
    printf '\nSign in at http://localhost:8080/sign-in as admin@shop.example with the password\n  %s\n(kept in %s/admin_password.txt)\n' "$password" "$dir"
else
    printf 'kept       %s/admin_password_hash.txt\n' "$dir"
fi

# The containers run as other users, so the files must be readable to them.
chmod 0755 "$dir"
chmod 0644 "$dir"/*.txt
