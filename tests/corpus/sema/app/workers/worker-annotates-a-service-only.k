// docs/design/55-workers.md, "Declaring a worker": `@Worker` goes on a
// service only (K3002).
import k.concurrency.Worker

@Worker
struct Point {
    let x: Int
}
