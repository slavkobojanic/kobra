/// A component whose test group is the second group of a file that sorts
/// before this one (`Greetings.test.k`).
public component Wave {
    @State var waved: Bool = false

    view {
        if waved {
            text { "Waved" }
        } else {
            button { "Wave" }
                .on(click: { self.waved = true })
        }
    }
}
