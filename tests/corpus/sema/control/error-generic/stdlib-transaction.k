// `k.db.Database.transaction` is error-generic: a block that can't throw needs
// no `try`, a throwing one does. `Database` is server-only, so this is a
// server service.
import k.db.Database

enum OrderError implements Error {
    case missing
}

func markPaid(id: Int) async throws OrderError -> Bool {
    return true
}

service Orders {
    @Inject let database: Database

    func touch() async -> Int {
        return await database.transaction { 1 }
    }

    func pay(id: Int) async throws OrderError -> Bool {
        return try await database.transaction {
            let marked = try await markPaid(id: id)
            marked
        }
    }
}
