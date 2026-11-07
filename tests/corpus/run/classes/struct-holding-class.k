// Copying a struct copies its class field's reference, so the instance stays shared.
class Journal {
    var lines: List<String> = []
}

struct Job {
    var name: String
    let log: Journal
}

func main() -> List<String> {
    let job = Job(name: "a", log: Journal())
    var other = job
    other.name = "b"
    other.log.lines.append("from b")
    return [job.name, other.name] + job.log.lines
}
