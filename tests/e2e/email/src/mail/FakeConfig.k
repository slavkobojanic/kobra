/// Where the harness's fake API is, to read the fake SMTP server's inbox.
public config FakeConfig {
    @Env("FAKE_URL") let url: Url
}
