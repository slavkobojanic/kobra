import k.db.Database
import k.rpc.RpcOnlyError
import k.server.JobQueue

public service WorkApi implements WorkProtocol {
    @Inject let jobs: JobQueue
    @Inject let database: Database
    @Inject let log: EntryRepository

    rpc record(text: String, delayMs: Int) async throws RpcOnlyError -> Bool {
        await jobs.enqueue(RecordJob.kind, payload: Note(text: text), delay: delayMs.milliseconds)

        return true
    }

    rpc recordInTransaction(text: String) async throws RpcOnlyError -> Bool {
        await database.transaction {
            await log.add("written with {text}")
            await jobs.enqueue(RecordJob.kind, payload: Note(text: text))
        }

        return true
    }

    rpc recordThenRollBack(text: String) async throws RpcOnlyError -> Bool {
        do {
            try await database.transaction {
                await jobs.enqueue(RecordJob.kind, payload: Note(text: text))

                throw WorkError.rolledBack
            }
        } catch _ {
            return false
        }

        return true
    }

    rpc flaky(text: String) async throws RpcOnlyError -> Bool {
        await jobs.enqueue(FlakyJob.kind, payload: Note(text: text))

        return true
    }

    rpc doomed(text: String) async throws RpcOnlyError -> Bool {
        await jobs.enqueue(DoomedJob.kind, payload: Note(text: text))

        return true
    }

    rpc refused(text: String) async throws RpcOnlyError -> Bool {
        await jobs.enqueue(RefusedJob.kind, payload: Note(text: text))

        return true
    }

    rpc entries() async throws RpcOnlyError -> List<String> {
        return await log.texts()
    }
}
