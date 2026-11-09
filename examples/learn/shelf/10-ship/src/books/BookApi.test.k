import accounts.AccountId
import k.auth.SessionManager
import k.crypto.Uuid
import k.rpc.RpcOnlyError

test BookApi {
    @Inject let api: BookProtocol
    @Inject let sessions: SessionManager

    test "every rpc needs someone signed in" {
        expect throws RpcOnlyError.rpcFailure(.rejected(.unauthenticated)) {
            try await api.list()
        }

        expect throws LookupError.rpcFailure(.rejected(.unauthenticated)) {
            try await api.get(id: Uuid.random())
        }
    }

    test "another person's book isn't found" {
        await sessions.signIn(userId: AccountId(value: Uuid.random()))
        let book = try await api.add(draft: BookDraft(title: "Piranesi", author: "Susanna Clarke"))

        await sessions.signIn(userId: AccountId(value: Uuid.random()))

        expect throws LookupError.notFound {
            try await api.get(id: book.id)
        }

        expect throws LookupError.notFound {
            try await api.review(id: book.id, review: BookReview(notes: "Mine now", rating: 1))
        }
    }

    test "the server checks a review's rules too" {
        await sessions.signIn(userId: AccountId(value: Uuid.random()))
        let book = try await api.add(draft: BookDraft(title: "Piranesi", author: "Susanna Clarke"))

        expect throws LookupError.rpcFailure(.invalid) {
            try await api.review(id: book.id, review: BookReview(notes: "", rating: 6))
        }
    }
}
