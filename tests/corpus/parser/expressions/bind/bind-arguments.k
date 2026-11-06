// `bind` is allowed directly as a labeled or positional argument value
func f() {
    TextField(text: bind form.email)
    Toggle(bind isOn)
}
