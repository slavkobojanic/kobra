// A payload field may have a default, checked against its type; a call may
// leave a defaulted field out, and must give the others.
enum Event {
    case posted(room: String, edited: Bool = false)
    case bad(count: Int = "none")
}

func make() -> List<Event> {
    return [Event.posted(room: "a"), Event.posted(room: "b", edited: true), Event.posted(edited: true)]
}
