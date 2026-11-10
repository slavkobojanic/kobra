// `break` in a `match` arm leaves the loop around the match, as anywhere
// else in the loop's body. A match on an enum with payloads is a JavaScript
// `switch`, whose own `break` would only leave the switch: the loop runs on
// (examples/huddle's test read a channel's events this way and hung).
enum Event {
    case posted(text: String)
    case typing(name: String)
}

func main() -> List<String> {
    let events = [Event.typing(name: "bob"), Event.posted(text: "one"), Event.posted(text: "two")]
    var seen: List<String> = []
    for event in events {
        match event {
            case .posted(let text) {
                seen.append(text)
                break
            }
            case .typing(let name) {
                seen.append(name)
            }
        }
    }
    var i = 0
    while i < 10 {
        i += 1
        match events[0] {
            case .typing(_) {
                if i == 3 {
                    break
                }
            }
            case .posted(_) { }
        }
    }
    seen.append("{i}")
    return seen
}
