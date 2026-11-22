---
kind: fix
---
Postgres text and enum columns are created with `COLLATE "C"`, so `sortedBy` and comparisons order text by code point on every database, as SQLite and Kobra's own `sortedBy` do, whatever the database's locale.
