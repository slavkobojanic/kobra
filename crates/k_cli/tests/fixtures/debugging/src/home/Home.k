public page Home {
    @Inject let greeter: GreeterClient

    @State var count: Int = 0
    @State var message: String = ""
    @Derived let label: String = "Clicked {count} times"

    func describe(n: Int) -> String {
        return match n {
            case 0 { "none" }
            case 1 { "one" }
            default { "many" }
        }
    }

    func greet() async {
        let text = await greeter.greeting(name: "Ada")
        message = text
    }

    func explode() {
        let empty: List<Int> = []
        count = empty[count]
    }

    view {
        heading(level: 1) { label }
        button { "Add" }
            .on(click: {
                let next = count + 1
                count = next
            })
        button { "Greet" }
            .on(click: greet)
        button { "Explode" }
            .on(click: explode)
        paragraph { "{describe(n: count)}: {message}" }
            .class(.note)
    }

    style {
        .note {
            color: red;
        }
    }
}
