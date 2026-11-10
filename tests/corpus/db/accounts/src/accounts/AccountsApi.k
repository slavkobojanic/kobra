import k.crypto.Uuid
import k.db.Database
import k.rpc.RpcOnlyError

/// The server side of `AccountsProtocol`.
public service AccountsApi implements AccountsProtocol {
    @Inject let accounts: AccountRepository
    @Inject let audits: AuditRepository
    @Inject let database: Database

    rpc signUp(id: Uuid, email: String, handle: String, name: String) async throws SignUpError -> Int {
        do {
            try await accounts.insert(Account(id: id, email: email, handle: handle, name: name))
        } catch .email {
            throw SignUpError.emailTaken
        } catch .handle {
            throw SignUpError.handleTaken
        }

        return await accounts.total()
    }

    rpc claim(id: Uuid, email: String, handle: String, name: String) async throws RpcOnlyError -> Bool {
        return await accounts.claim(Account(id: id, email: email, handle: handle, name: name))
    }

    rpc renameIfFree(id: Uuid, handle: String) async throws RpcOnlyError -> Bool {
        guard let account = await accounts.byId(id) else {
            return false
        }

        var renamed = account
        renamed.handle = handle

        return await accounts.renameIfFree(renamed)
    }

    rpc rename(id: Uuid, handle: String) async throws SignUpError -> String {
        guard let account = await accounts.byId(id) else {
            return "missing"
        }

        var renamed = account
        renamed.handle = handle

        do {
            try await accounts.update(renamed)
        } catch .email {
            throw SignUpError.emailTaken
        } catch .handle {
            throw SignUpError.handleTaken
        }

        guard let stored = await accounts.byId(id) else {
            return "gone"
        }

        return stored.handle
    }

    rpc failAfterWriting(id: Uuid) async throws SignUpError -> Int {
        try await database.transaction {
            await audits.insert(Audit(id: id, message: "written then rolled back"))

            throw SignUpError.emailTaken
        }

        return 0
    }

    rpc nested(outer: Uuid, inner: Uuid) async throws RpcOnlyError -> List<String> {
        await database.transaction {
            await audits.insert(Audit(id: outer, message: "outer kept"))

            do {
                try await database.transaction {
                    await audits.insert(Audit(id: inner, message: "inner rolled back"))

                    throw SignUpError.handleTaken
                }
            } catch _ {
                // The savepoint is gone; the outer write stays.
            }
        }

        return await audits.messages()
    }

    rpc audit() async throws RpcOnlyError -> List<String> {
        return await audits.messages()
    }

    rpc healthy() async throws RpcOnlyError -> Bool {
        return await database.ping(timeout: 2.seconds)
    }
}
