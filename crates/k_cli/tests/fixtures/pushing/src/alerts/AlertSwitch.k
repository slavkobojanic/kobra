/// "Notify me": subscribes from the click and says what happened.
public component AlertSwitch {
    @Inject let alerts: PushAlerts

    @State var said: String = ""

    view {
        if alerts.canOffer {
            button { "Notify me" }
                .on(click: turnOn)
        }

        paragraph { said }
            .role(.status)
    }

    func turnOn() async {
        said = await alerts.turnOn()
    }
}
