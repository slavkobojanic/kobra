import k.server.Job
import k.server.JobError

/// Sends an account's monthly statement (`storage.lock`).
@JobName("statement")
public service StatementJob implements Job<Statement> {
    func run(payload: Statement) async throws JobError {}
}
