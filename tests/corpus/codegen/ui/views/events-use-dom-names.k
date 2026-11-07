// `.on(…)` maps K event names to DOM event names: `mouseEnter` → `mouseenter`, and the handler is
// wrapped by `$k.on`, which batches (codegen.md §10, runtime.md §5.4).
component Hover {
    @State var log: List<String> = []

    view {
        box {
            button { "Hover me" }
                .on(click: { log.append("click") })
                .on(focus: { log.append("focus") })
                .on(blur: { log.append("blur") })

            box { "zone" }
                .on(mouseEnter: { log.append("enter") })
                .on(mouseLeave: { log.append("leave") })
        }
    }
}
