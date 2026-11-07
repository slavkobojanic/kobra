#!/usr/bin/env bash
#
# End-to-end test of the whole system: builds both images, starts the stack
# with Docker Compose (Postgres, Mailpit, paybox, two shop replicas behind
# Caddy), exercises it over HTTP with curl exactly as a browser and an
# administrator would, and tears everything down again, volumes included,
# whether the run passes or fails. No account is needed anywhere.
#
# Usage: scripts/e2e.sh
# Needs: docker (with Compose v2), curl, openssl, jq; argon2, or docker to
# run it (scripts/init-secrets.sh).
#
# The run never uses secrets/: scripts/init-secrets.sh generates every
# secret, including a random admin password and its hash, in a temporary
# directory, and an override file points Compose at them.
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

# Calls an RPC as the shop's pages do: rpc <Package.Protocol/Rpc> <json> [cookie jar].
# Prints the status code.
rpc() {
    curl --silent --show-error --output "$work_dir/body" --write-out '%{http_code}' \
        --header 'Content-Type: application/json' --header 'Connect-Protocol-Version: 1' \
        --header "Origin: $base_url" \
        ${3:+--cookie-jar "$3" --cookie "$3"} \
        --data "$2" "$base_url/$1"
}

expect_status() {
    [[ "$1" == "$2" ]] || { cat "$work_dir/body" >&2; fail "$3: expected HTTP $2, got $1"; }
    pass "$3 (HTTP $1)"
}

# The number of emails Mailpit holds for the customer.
emails() {
    curl --silent --get --data-urlencode "query=to:\"$customer\"" "$mail_api/search" | jq '.messages_count'
}

cleanup() {
    local status=$?

    if (( status != 0 )); then
        printf '\nLogs:\n' >&2
        compose logs paybox shop proxy --tail 60 >&2 || true
    fi

    compose down --volumes --remove-orphans >/dev/null 2>&1 || true
    rm -rf "$work_dir"

    exit "$status"
}

for tool in docker curl openssl jq; do
    command -v "$tool" >/dev/null || { echo "e2e: $tool is required" >&2; exit 2; }
done

trap cleanup EXIT

step "Generating secrets for this run"
scripts/init-secrets.sh "$secrets_dir" >/dev/null
admin_password="$(cat "$secrets_dir/admin_password.txt")"
readonly admin_password

{
    printf 'secrets:\n'
    for name in db_password paybox_db_password admin_password_hash paybox_service_token shop_service_token; do
        printf '  %s:\n    file: %s/%s.txt\n' "$name" "$secrets_dir" "$name"
    done
} > "$compose_override"
pass "secrets generated in $secrets_dir"

step "Starting the stack (build, migrate, wait for health checks)"
compose up --build --detach --wait --wait-timeout 900

for attempt in $(seq 1 30); do
    if curl --silent --fail "$base_url/health" >/dev/null; then
        break
    fi
    (( attempt < 30 )) || fail "the shop never answered /health through the proxy"
    sleep 1
done
pass "the shop is healthy behind the proxy"
[[ "$(compose ps --quiet shop | wc -l | tr -d ' ')" == 2 ]] || fail "expected two shop replicas"
pass "two shop replicas are running"

step "Loading the demo catalog"
# The catalog isn't a migration (see shop/scripts/demo-catalog.sql), so the
# run inserts it once the shop has created the tables.
compose exec -T db psql --username=shop --dbname=shop --set ON_ERROR_STOP=1 --quiet \
    < shop/scripts/demo-catalog.sql >/dev/null
pass "three demo products"

step "Storefront and SEO"
expect_status "$(get /)" 200 "home page"
expect_body "K Mug"
expect_body "Payments are handled by paybox"
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
expect_status "$status" 200 "placeOrder (the shop registered with paybox)"
order_id="$(jq --raw-output '.orderId' "$work_dir/body")"
payment_url="$(jq --raw-output '.paymentUrl' "$work_dir/body")"
[[ "$payment_url" == "$base_url/payment/$order_id" ]] || fail "unexpected payment URL: $payment_url"
pass "order $order_id sends the customer to the shop's card form"

status="$(rpc shop.store.StoreProtocol/PlaceOrder \
    "{\"request\":{\"productSlug\":\"k-mug\",\"quantity\":0,\"email\":\"$customer\"}}")"
expect_status "$status" 400 "an order for zero units is rejected by validation"

expect_status "$(get "/payment/$order_id")" 200 "payment page"
expect_body "Pay \$48.00"

step "Paying through paybox"
pay() {
    rpc shop.paybox.CardProtocol/Pay "{\"orderId\":\"$order_id\",\"card\":{\"number\":\"$1\"}}"
}
expect_status "$(pay "4000 0000 0000 0002")" 400 "the decline card is declined"
expect_body "Your card was declined."
expect_status "$(pay "4000 0000 0000 9995")" 400 "the insufficient-funds card is declined"
expect_body "insufficient funds"
expect_status "$(pay "1234 5678")" 400 "a number that isn't a card is refused"
expect_body "invalidCard"
expect_status "$(pay "4242 4242 4242 4242")" 200 "the success card is charged"
# Paying again before paybox's event arrives charges nothing more: the order
# id is the charge's reference, and paybox answers with the first charge.
expect_status "$(pay "4242 4242 4242 4242")" 200 "a repeated payment is answered with the first charge"

step "Paybox's event confirms the payment (a background job calling the shop)"
for attempt in $(seq 1 30); do
    get "/receipt/$order_id" >/dev/null
    grep --quiet --fixed-strings "Payment received" "$work_dir/body" && break
    (( attempt < 30 )) || fail "the receipt never showed the payment"
    sleep 1
done
pass "the receipt shows the payment"
reference="$(compose exec -T db psql --username=shop --dbname=shop --tuples-only --no-align \
    --command "SELECT payment_reference FROM \"order\" WHERE id = '$order_id'")"
[[ "$reference" == ch_* ]] || fail "the order's payment reference is not paybox's charge: $reference"
pass "the order carries paybox's charge id ($reference)"
charges="$(compose exec -T db psql --username=shop --dbname=paybox --tuples-only --no-align \
    --command "SELECT count(*) FROM charge_record WHERE key = 'shop/$order_id'")"
[[ "$charges" == 1 ]] || fail "expected one charge in paybox's database, found $charges"
pass "paybox charged the order once"
expect_status "$(pay "4242 4242 4242 4242")" 400 "a paid order can't be paid again"
expect_body "notAwaitingPayment"

step "Confirmation email (background job over SMTP, to Mailpit)"
messages=0
for attempt in $(seq 1 30); do
    messages="$(emails)"
    (( messages == 0 )) || break
    sleep 1
done
(( messages == 1 )) || fail "expected 1 confirmation email to $customer, found $messages"
sleep 3
(( "$(emails)" == 1 )) || fail "a second confirmation email was sent"
pass "exactly one confirmation email was sent"

step "Stripe is off by default"
status="$(curl --silent --output "$work_dir/body" --write-out '%{http_code}' \
    --header 'Content-Type: application/json' --data '{}' "$base_url/webhooks/stripe")"
expect_status "$status" 404 "the Stripe webhook refuses deliveries"

step "Admin page guards"
status="$(curl --silent --output /dev/null --write-out '%{http_code} %{redirect_url}' "$base_url/admin")"
[[ "$status" == "303 $base_url/sign-in"* ]] || fail "expected a redirect to sign in, got: $status"
pass "a visitor without a session is redirected to sign in"

credentials="{\"request\":{\"email\":\"$admin_email\",\"password\":\"$admin_password\"}}"
status="$(rpc shop.admin.AdminProtocol/SignIn "$credentials" "$work_dir/admin-cookies")"
expect_status "$status" 200 "admin sign-in"
# The session lives in Postgres, so whichever replica answers knows it.
for attempt in 1 2 3 4; do
    expect_status "$(get /admin "$work_dir/admin-cookies")" 200 "admin dashboard with a session ($attempt)"
done
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
