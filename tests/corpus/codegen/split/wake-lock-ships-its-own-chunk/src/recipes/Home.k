import k.device.KeepScreenAwake

public page Home at "/" {
    @State var cooking: Bool = false
    @State var note: String = ""

    view {
        button { "Cook" }
            .on(click: { cooking = not cooking })

        if cooking {
            KeepScreenAwake()
                .on(failed: { reason => note = "{reason}" })
        }

        text { note }
    }
}
