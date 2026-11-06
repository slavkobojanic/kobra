// A job has no session (30-background-jobs.md): a job whose dependency graph
// reaches `Session` is K4009 at the job's name, with a note naming the path,
// directly or through another service.
import k.auth.Session
import k.server.Job
import k.server.JobError

struct Mail implements Codable {
    let to: String
}

service Audit {
    @Inject let session: Session
}

service MailJob implements Job<Mail> {
    @Inject let audit: Audit

    func run(payload: Mail) async throws JobError {
    }
}

service DirectJob implements Job<Mail> {
    @Inject let session: Session

    func run(payload: Mail) async throws JobError {
    }
}

service FineJob implements Job<Mail> {
    func run(payload: Mail) async throws JobError {
    }
}
