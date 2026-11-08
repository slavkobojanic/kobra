import k.observe.Log

/// A page with an async event handler and an inline `form.handle` action.
public page Compose {
    @Inject let desk: Desk
    @Form var draft: Draft = .empty

    view {
        Picker()
            .on(picked: { value =>
                do {
                    let name = try await desk.lookup(id: value)
                    Log.info("picked {name}")
                } catch _ {
                    Log.info("pick failed")
                }
            })

        form {
            textField(value: bind draft.text)
                .label("Text")

            button { "Send" }
                .type(.submit)
        }
        .on(submit: draft.handle({
            do {
                let saved = try await desk.save(text: draft.value.text)
                Log.info("sent {saved}")
            } catch _ {
                Log.info("send failed")
            }
        }))
    }
}
