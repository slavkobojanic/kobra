/// Where another service of the same system runs, and the secret its calls
/// are signed with (`docs/design/40-workspaces-and-services.md`). A
/// `config` implements it with `@Env` fields `url: Url` and
/// `token: Secret<String>`, and optionally `timeout: Duration` (10 seconds
/// when it has none); `use remote(Config) for Protocol` in `App.k` then
/// calls `Protocol` at that address, from the server only.
public interface RemoteService {}
