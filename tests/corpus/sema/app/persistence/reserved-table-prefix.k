// The `k_` table prefix is reserved for the stdlib: `entity KCache` would be table `k_cache`.
entity KCache {
    @Id let id: Int
}

entity Cache {
    @Id let id: Int
}
