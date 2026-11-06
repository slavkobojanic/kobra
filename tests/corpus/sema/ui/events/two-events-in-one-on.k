// One event per `.on(…)` call: a second label is K3108 with a fix that splits the
// call (04-components.md, "Events").
component Field {
    @State var query: String = ""

    func start() {
    }

    func stop() {
    }

    view {
        textField(value: bind query)
            .on(focus: start, blur: stop)
    }
}
