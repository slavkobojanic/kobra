test Searchable {
    test "a blank query matches everything" {
        let item = SearchableItem(pieces: ["Groceries"])

        expect item.matches(query: "")
        expect item.matches(query: "   ")
    }

    test "matching ignores letter case and surrounding spaces" {
        let item = SearchableItem(pieces: ["Groceries", "Milk and eggs"])

        expect item.matches(query: "  EGGS ")
    }

    test "a query must appear within a single piece" {
        let item = SearchableItem(pieces: ["Groceries", "Milk and eggs"])

        expect not item.matches(query: "groceries milk")
    }
}

/// The smallest possible conformance, so these tests exercise only the
/// default `matches(query:)`.
struct SearchableItem implements Searchable {
    let pieces: List<String>

    func searchableText() -> List<String> {
        return pieces
    }
}
