// A `match` used as a statement runs one arm for its effects.
import k.observe.Log

enum Event {
    case click(x: Int)
    case key(String)
}

func handle(event: Event) {
    match event {
        case .click(let x) {
            Log.info("click at {x}")
        }
        case .key(let k) {
            Log.info("key {k}")
        }
    }
}

func main() {
    handle(event: .click(x: 3))
    handle(event: .key("a"))
}
