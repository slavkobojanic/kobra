import k.auth.PasskeyCredential

/// The fixture's passkeys, one list per server process (`@State`, so
/// every isolate of `k run` sees it). The notes example keeps them in an
/// entity instead.
@Scope(.server)
public service PasskeyBook {
    @State var stored: List<StoredPasskey> = []

    /// Stores a verified credential for `owner`.
    func add(owner: String, credential: PasskeyCredential) {
        stored.append(StoredPasskey(owner: owner, credential: credential))
    }

    /// The passkey with credential id `id`.
    func find(id: String) -> StoredPasskey? {
        return stored.firstWhere { s => s.credential.id == id }
    }

    /// Whether `owner` has a passkey.
    func has(owner: String) -> Bool {
        return stored.contains(where: { s => s.owner == owner })
    }

    /// Writes back a credential after a sign-in: its new counter.
    func update(_ credential: PasskeyCredential) {
        var next: List<StoredPasskey> = []
        for s in stored {
            if s.credential.id == credential.id {
                next.append(StoredPasskey(owner: s.owner, credential: credential))
            } else {
                next.append(s)
            }
        }
        stored = next
    }
}
