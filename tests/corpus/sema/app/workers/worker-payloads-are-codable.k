// docs/design/55-workers.md, "Methods": parameters, results and errors cross
// to the worker through their codecs, so they are `Codable` (K4039); a stream
// result isn't supported yet.
import k.concurrency.Worker
import k.concurrency.WorkerFailure
import k.concurrency.WorkerOnlyError

struct Shape {
    let sides: Int
}

enum DrawError implements Error {
    case blank
    case workerFailure(WorkerFailure)
}

@Worker
service Drawing {
    func area(of shape: Shape) async throws WorkerOnlyError -> Int {
        return shape.sides
    }

    func outline(sides: Int) async throws WorkerOnlyError -> Shape {
        return Shape(sides: sides)
    }

    func relay(ticks: Stream<Int>) async throws WorkerOnlyError -> Stream<Int> {
        return ticks
    }

    func draw(sides: Int) async throws DrawError -> Int {
        return sides
    }
}
