#!/bin/sh
set -eu

: "${APP_DB_PASSWORD:?Set a separate application database password}"
if [ "$APP_DB_PASSWORD" = "$POSTGRES_PASSWORD" ]; then
    printf '%s\n' 'APP_DB_PASSWORD must differ from POSTGRES_PASSWORD.' >&2
    exit 1
fi

# psql quotes password as an SQL literal and database as an SQL identifier.
# This script runs only when PostgreSQL initializes a new data directory.
psql --set=ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" \
    --set=app_password="$APP_DB_PASSWORD" --set=database_name="$POSTGRES_DB" <<'SQL'
CREATE ROLE market_app WITH LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS PASSWORD :'app_password';
REVOKE ALL ON DATABASE :"database_name" FROM PUBLIC;
GRANT CONNECT ON DATABASE :"database_name" TO market_app;
REVOKE ALL ON SCHEMA public FROM PUBLIC;
ALTER SCHEMA public OWNER TO market_app;
GRANT USAGE, CREATE ON SCHEMA public TO market_app;
SQL
