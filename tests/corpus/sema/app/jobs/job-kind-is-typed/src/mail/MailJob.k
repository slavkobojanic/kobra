// Every job gets a generated `static let kind: JobKind<Payload>`, typed by
// its `Job<…>` payload, so a generic call such as `payloads(of:)` infers
// the payload from it, and with it the codec the payload needs.
import k.server.Job
import k.server.JobError

/// What the job sends.
struct Mail implements Codable {
    let to: String
}

/// Sends mail.
service MailJob implements Job<Mail> {
    func run(payload: Mail) async throws JobError {
        let kind = MailJob.kind
        //  ^^^^ type: k.server.JobKind<mail.Mail>
        let name: String = MailJob.kind.name
        let wrong: Int = MailJob.kind
    }
}
