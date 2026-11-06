// Two config fields naming the same variable: the later one is reported at its name.
config MailConfig {
    @Env("SMTP_HOST") let host: String
}

config BackupMailConfig {
    @Env("SMTP_HOST") let host: String
    @Env("BACKUP_SMTP_PORT") let port: Int = 587
}
