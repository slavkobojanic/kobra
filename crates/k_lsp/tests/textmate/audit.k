/// A board of cards, see [`Card`] and `count`.
/// - Parameter title: the board's name, "quoted" and {braced}
@Env("BOARD_TITLE") let title: Secret<String>

// A line comment with a quote " and an apostrophe '
/* A block comment, /* nested */ with "quotes" */

public struct Box<T> implements Codable where T implements Equatable {
    let value: T
}

func label(count: Int, items: List<String>) -> String {
    let doubled = "{{literal}} and \"escaped\" and \u{1F600}"
    let nested = "{ items.map { item => item.count }.count } left, and is 2"
    let inner = "outer {"inner {count} words"} done"
    return "{count} cards"
}

component Board {
    view {
        heading { "The board's {title} cards" }
    }

    style {
        .card { color: .muted; padding: 16px; }
    }
}

test "if the board is empty, it shows a default message" {
    expect label(count: 0, items: []) == "0 cards"
}

test Board {
    test "it doesn't crash" {
        expect true
    }
}
