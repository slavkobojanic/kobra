#!/usr/bin/env bash
#
# End-to-end test for the shop: builds the image, starts the whole stack with
# Docker Compose, exercises it over HTTP with curl exactly as a browser, Stripe
# and an administrator would, and tears everything down again, volumes
# included, whether the run passes or fails.
#
# Usage: scripts/e2e.sh
# Needs: docker (with Compose v2), curl, openssl, jq, argon2.
#
# The run never uses secrets/: it generates every secret, including a random
# admin password and its Argon2id hash, in a temporary directory, and points
# Compose at them with an override file. The committed secrets/*.example
# files are placeholders that deliberately fail validation
# (docs/design/25-config-and-secrets.md#per-environment).
#
# RPCs are called with Connect's JSON encoding: POST /<package>.<Protocol>/<Rpc>
# (see docs/design/28-rpc-wire-format.md).

set -euo pipefail

cd "$(dirname "$0")/.."

readonly base_url="http://localhost:8080"
readonly mail_api="http://localhost:8025/api/v1"
readonly project="k-shop-e2e"
readonly customer="e2e-$(date +%s)@example.com"
readonly admin_email="admin@shop.example"     # SHOP_ADMIN_EMAIL in docker-compose.yml

work_dir="$(mktemp -d)"
readonly work_dir
readonly secrets_dir="$work_dir/secrets"
readonly compose_override="$work_dir/compose.secrets.yml"

compose() {
    docker compose --project-name "$project" --file docker-compose.yml --file "$compose_override" "$@"
}

step() {
    printf '\n==> %s\n' "$*"
}

pass() {
    printf '    ok  %s\n' "$*"
}

fail() {
    printf '    FAIL  %s\n' "$*" >&2
    exit 1
}

# Checks that the last response body ($work_dir/body) contains a string.
expect_body() {
    grep --quiet --fixed-strings -- "$1" "$work_dir/body" || fail "response body does not contain: $1"
    pass "body contains: $1"
}

# GET <path> [cookie jar]. Prints the status code; the body goes to $work_dir/body.
get() {
    curl --silent --show-error --output "$work_dir/body" --write-out '%{http_code}' \
        ${2:+--cookie "$2"} "$base_url$1"
}

# Calls an RPC: rpc <Package.Protocol/Rpc> <json> [cookie jar]. Prints the status code.
rpc() {
    curl --silent --show-error --output "$work_dir/body" --write-out '%{http_code}' \
        --header 'Content-Type: application/json' --header 'Connect-Protocol-Version: 1' \
        ${3:+--cookie-jar "$3" --cookie "$3"} \
        --data "$2" "$base_url/$1"
}

# POSTs a webhook delivery: webhook <payload> [Stripe-Signature]. Prints the status code.
webhook() {
    curl --silent --show-error --output "$work_dir/body" --write-out '%{http_code}' \
        --header 'Content-Type: application/json' \
        ${2:+--header "Stripe-Signature: $2"} \
        --data "$1" "$base_url/webhooks/stripe"
}

# Signs a payload the way Stripe does: sign <payload> <unix time> [secret].
sign() {
    local digest
    digest="$(printf '%s.%s' "$2" "$1" | openssl dgst -sha256 -hmac "${3:-$webhook_secret}" -hex | sed 's/^.*= //')"
    printf 't=%s,v1=%s' "$2" "$digest"
}

expect_status() {
    [[ "$1" == "$2" ]] || { cat "$work_dir/body" >&2; fail "$3: expected HTTP $2, got $1"; }
    pass "$3 (HTTP $1)"
}

cleanup() {
    local status=$?

    if (( status != 0 )); then
        printf '\nApp logs:\n' >&2
        compose logs app --tail 80 >&2 || true
    fi

    compose down --volumes --remove-orphans >/dev/null 2>&1 || true
    rm -rf "$work_dir"

    exit "$status"
}

# Writes one generated secret where the Compose override points: secret <name> <value>.
secret() {
    printf '%s\n' "$2" > "$secrets_dir/$1.txt"
}

for tool in docker curl openssl jq argon2; do
    command -v "$tool" >/dev/null || { echo "e2e: $tool is required" >&2; exit 2; }
done

trap cleanup EXIT

step "Generating secrets for this run"
# This stack talks to stripe-mock, never to Stripe, so any sk_test_ key works.
mkdir -p "$secrets_dir"

admin_password="$(openssl rand -hex 16)"
readonly admin_password
webhook_secret="whsec_$(openssl rand -hex 24)"
readonly webhook_secret

secret db_password "$(openssl rand -hex 24)"
secret stripe_api_key "sk_test_e2e_$(openssl rand -hex 12)"
secret stripe_webhook_secret "$webhook_secret"
# Argon2id with 64 MiB of memory (-m 16 is 2^16 KiB), 3 iterations and 4
# lanes, printed as a PHC string, the format AdminConfig requires.
secret admin_password_hash "$(printf '%s' "$admin_password" | argon2 "$(openssl rand -hex 16)" -id -t 3 -m 16 -p 4 -e)"

# The containers run as other users, so the files must be readable to them.
chmod 0755 "$secrets_dir"
chmod 0644 "$secrets_dir"/*.txt

cat > "$compose_override" <<YAML
secrets:
  db_password:
    file: $secrets_dir/db_password.txt
  stripe_api_key:
    file: $secrets_dir/stripe_api_key.txt
  stripe_webhook_secret:
    file: $secrets_dir/stripe_webhook_secret.txt
  admin_password_hash:
    file: $secrets_dir/admin_password_hash.txt
YAML
pass "secrets generated in $secrets_dir"

step "Starting the stack (build, migrate, wait for health checks)"
compose up --build --detach --wait --wait-timeout 240

for attempt in $(seq 1 30); do
    if curl --silent --fail "$base_url/health" >/dev/null; then
        break
    fi
    (( attempt < 30 )) || fail "the app never answered /health from the host"
    sleep 1
done
pass "app is healthy"

step "Loading the demo catalog"
# The catalog isn't a migration (see scripts/demo-catalog.sql), so the run
# inserts it once the app has created the tables.
compose exec -T db psql --username=shop --dbname=shop --set ON_ERROR_STOP=1 --quiet \
    < scripts/demo-catalog.sql >/dev/null
pass "three demo products"

step "Storefront and SEO"
expect_status "$(get /)" 200 "home page"
expect_body "K Mug"
expect_status "$(get /products/k-mug)" 200 "product page"
expect_body '<link rel="canonical" href="http://localhost:8080/products/k-mug">'
expect_body '<script type="application/ld+json">'
expect_body '"@type":"Product"'
expect_status "$(get /products/sticker-pack)" 200 "sold-out product page"
expect_body "Sold out"
expect_status "$(get /products/no-such-product)" 404 "an unknown product answers 404"
expect_body "Product not found"

step "Placing an order"
status="$(rpc shop.store.StoreProtocol/PlaceOrder \
    "{\"request\":{\"productSlug\":\"k-mug\",\"quantity\":2,\"email\":\"$customer\"}}")"
expect_status "$status" 200 "placeOrder"
order_id="$(jq --raw-output '.orderId' "$work_dir/body")"
payment_url="$(jq --raw-output '.paymentUrl' "$work_dir/body")"
[[ "$payment_url" == https://checkout.stripe.com/* ]] || fail "unexpected payment URL: $payment_url"
pass "order $order_id sends the customer to Stripe Checkout"

status="$(rpc shop.store.StoreProtocol/PlaceOrder \
    "{\"request\":{\"productSlug\":\"k-mug\",\"quantity\":0,\"email\":\"$customer\"}}")"
expect_status "$status" 400 "an order for zero units is rejected by validation"

expect_status "$(get "/receipt/$order_id")" 200 "receipt"
expect_body "Awaiting payment"

step "Stripe webhook"
now="$(date +%s)"
payload="{\"id\":\"evt_e2e_1\",\"type\":\"checkout.session.completed\",\"data\":{\"object\":{\"id\":\"cs_e2e_1\",\"client_reference_id\":\"$order_id\",\"payment_status\":\"paid\",\"amount_total\":4800,\"currency\":\"usd\"}}}"

expect_status "$(webhook "$payload")" 400 "an unsigned delivery is refused"
expect_status "$(webhook "$payload" "$(sign "$payload" "$now" whsec_attacker)")" 400 "a forged signature is refused"
expect_status "$(webhook "$payload" "$(sign "$payload" "$(( now - 600 ))")")" 400 "a ten-minute-old signature is refused"
expect_status "$(webhook "$payload" "$(sign "$payload" "$now")")" 204 "a signed delivery is accepted"
expect_status "$(webhook "$payload" "$(sign "$payload" "$now")")" 204 "a repeated delivery is acknowledged"

expect_status "$(get "/receipt/$order_id")" 200 "receipt after payment"
expect_body "Paid"

step "Confirmation email (background job over SMTP)"
messages=0
for attempt in $(seq 1 30); do
    messages="$(curl --silent --get --data-urlencode "query=to:\"$customer\"" "$mail_api/search" | jq '.messages_count')"
    (( messages == 0 )) || break
    sleep 1
done
(( messages == 1 )) || fail "expected 1 confirmation email to $customer, found $messages"
sleep 3
messages="$(curl --silent --get --data-urlencode "query=to:\"$customer\"" "$mail_api/search" | jq '.messages_count')"
(( messages == 1 )) || fail "the repeated webhook sent a second email"
pass "exactly one confirmation email was sent"

step "Admin page guards"
status="$(curl --silent --output /dev/null --write-out '%{http_code} %{redirect_url}' "$base_url/admin")"
[[ "$status" == "303 $base_url/sign-in"* ]] || fail "expected a redirect to sign in, got: $status"
pass "a visitor without a session is redirected to sign in"

expect_status "$(rpc shop.admin.AdminProtocol/SignIn \
    "{\"request\":{\"email\":\"$admin_email\",\"password\":\"$admin_password\"}}" \
    "$work_dir/admin-cookies")" 200 "admin sign-in"
expect_status "$(get /admin "$work_dir/admin-cookies")" 200 "admin dashboard with a session"
expect_body "$customer"

step "Rate limit on sign-in"
saw_limit=false
for attempt in $(seq 1 6); do
    status="$(rpc shop.admin.AdminProtocol/SignIn \
        "{\"request\":{\"email\":\"$admin_email\",\"password\":\"wrong-$attempt\"}}")"
    if [[ "$status" == 429 ]]; then
        saw_limit=true
        break
    fi
    [[ "$status" == 400 ]] || fail "wrong password attempt $attempt: expected HTTP 400 or 429, got $status"
done
[[ "$saw_limit" == true ]] || fail "six sign-ins within a minute were never rate limited"
pass "repeated sign-in attempts are rate limited (HTTP 429)"

printf '\nAll end-to-end checks passed.\n'
