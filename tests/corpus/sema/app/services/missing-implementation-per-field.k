// Every `@Inject` field of an interface without an implementation is reported, at its type.
interface Mailer {
    func send(text: String) async
}

service Welcome {
    @Inject let mailer: Mailer
}

service Reminders {
    @Inject let mailer: Mailer
}
