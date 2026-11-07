/// Server-only: a config, with a secret.
public config LedgerSecrets {
    @Env("SPLIT_SECRET_TOKEN") let token: Secret<String>
    @Env("SPLIT_LEDGER_NAME") let ledgerName: String
    @Env("SPLIT_DEFAULTED") let defaulted: String = "SPLIT_CONFIG_DEFAULT_77aa"
}
