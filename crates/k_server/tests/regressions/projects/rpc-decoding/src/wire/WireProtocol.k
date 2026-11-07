import k.crypto.Uuid
import k.rpc.RpcOnlyError
import k.text.EmailAddress

/// Parameters of typed values.
public protocol WireProtocol {
    /// An `EmailAddress` parameter.
    rpc mail(to address: EmailAddress) throws RpcOnlyError -> String

    /// A `Url` parameter.
    rpc site(url: Url) throws RpcOnlyError -> String

    /// A `Uuid` parameter.
    rpc lookup(id: Uuid) throws RpcOnlyError -> String

    /// A `Color` parameter (it reaches `style` attributes as a custom property).
    rpc paint(color: Color) throws RpcOnlyError -> String

    /// A map with non-string keys travels as pairs.
    rpc count(scores: Map<Int, String>) throws RpcOnlyError -> Int

    /// A `Validatable` parameter with `@Email` and `@Matches`.
    rpc signup(request: Signup) throws RpcOnlyError -> String

    /// `@Length` counts characters.
    rpc nick(request: Nick) throws RpcOnlyError -> Int

    /// A list of `Validatable` values.
    rpc signupAll(requests: List<Signup>) throws RpcOnlyError -> Int
}
