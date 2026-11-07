#!/bin/sh
#
# Runs once, when Postgres initialises an empty data directory
# (/docker-entrypoint-initdb.d): gives paybox its own role and database next
# to the shop's. The two services never share tables; each `dist/app`
# migrates its own database at startup.
#
# The password comes from the paybox_db_password secret, the same file
# paybox reads through DATABASE_PASSWORD_FILE.

set -eu

password="$(cat /run/secrets/paybox_db_password)"

psql --variable=ON_ERROR_STOP=1 --username="$POSTGRES_USER" --dbname="$POSTGRES_DB" \
    --set=password="$password" <<'SQL'
CREATE ROLE paybox LOGIN PASSWORD :'password';
CREATE DATABASE paybox OWNER paybox;
SQL
