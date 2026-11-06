// A mutating method called on a prop (`List.append`) is a write: K3014 at the
// receiver. A non-mutating call (`appending`) is fine.
component Tags {
    @Prop let tags: List<String>
    @State var extra: List<String> = []

    func add() {
        tags.append("new")
        extra = tags.appending("new")
    }

    view {
        text { tags.joined(separator: ", ") }
    }
}
