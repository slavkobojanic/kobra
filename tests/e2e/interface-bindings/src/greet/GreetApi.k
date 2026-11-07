import k.rpc.RpcOnlyError
import k.server.PublicUrls

public service GreetApi implements GreetProtocol {
    @Inject let greeter: Greeter
    @Inject let stamp: Stamp
    @Inject let urls: PublicUrls

    rpc greet(name: String) async throws RpcOnlyError -> Greeting {
        return Greeting(text: "{greeter.greet(name: name)} {stamp.stamp()}", home: urls.absolute(.home))
    }
}
