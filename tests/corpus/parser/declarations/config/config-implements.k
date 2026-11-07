// A config may implement interfaces, such as `RemoteService` (40-workspaces-and-services.md).
public config PayboxService implements RemoteService {
    @Env("PAYBOX_URL") let url: Url
}
