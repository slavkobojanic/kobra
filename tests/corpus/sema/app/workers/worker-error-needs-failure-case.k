// docs/design/55-workers.md, "Methods": a worker method's error is
// `WorkerOnlyError` or a Codable enum with exactly one
// `workerFailure(WorkerFailure)` case (K4038); the fix adds the case.
import k.concurrency.Worker

enum ParseError implements Error, Codable {
    case empty
}

@Worker
service Parser {
    func parse(text: String) async throws ParseError -> Int {
        guard text.isNotEmpty else {
            throw .empty
        }
        return text.count
    }
}
