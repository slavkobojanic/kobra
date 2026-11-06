// A job's payload is stored in the queue, so it is `Codable`: `Job`'s
// `where Payload implements Codable` is K2009 at the type argument.
import k.server.Job
import k.server.JobError

struct Mail {
    let to: String
}

service MailJob implements Job<Mail> {
    func run(payload: Mail) async throws JobError {
    }
}
