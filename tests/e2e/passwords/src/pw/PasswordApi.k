import k.crypto.Password
import k.rpc.RpcOnlyError

public service PasswordApi implements PasswordProtocol {
    rpc roundTrip(password: String, other: String) async throws RpcOnlyError -> String {
        let first = await Password.hash(password: password)
        let second = await Password.hash(password: password)
        let same = await Password.verify(password: password, hash: first)
        let wrong = await Password.verify(password: other, hash: first)
        let salted = first != second
        let phc = first.startsWith("$argon2id$v=19$m=19456,t=2,p=1$")
        return "same={same} wrong={wrong} salted={salted} phc={phc}"
    }

    rpc unusable(password: String) async throws RpcOnlyError -> Bool {
        return await Password.verify(password: password, hash: Password.unusableHash)
    }

    rpc against(password: String, hash: String) async throws RpcOnlyError -> Bool {
        return await Password.verify(password: password, hash: hash)
    }
}
