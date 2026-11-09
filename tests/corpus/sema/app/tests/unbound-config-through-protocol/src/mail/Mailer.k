// The browser side, which reaches the config only through the protocol.
public service Mailer {
    @Inject let mail: MailProtocol
}
