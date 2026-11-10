import accounts.AccountId
import k.auth.SessionManager
import k.crypto.Uuid
import k.test.TestPeers

test PasswordRooms {
    @Inject let sessions: SessionManager
    @Inject let peers: TestPeers
    @Inject let rooms: ChatRoomRepository

    /// Bob creates `#vault` with the password "open sesame".
    func bobsVault() async {
        let bob = await peers.signIn(userId: AccountId(name: "bob"))
        let lobby: LobbyProtocol = bob.client()
        // A function throws one error type, and these are two.
        do {
            let channel = try await lobby.lobby()
            try await channel.send(.create(name: "vault", password: "open sesame"))
        } catch _ {
            expect false
        }
    }

    test "the password is stored as an Argon2id hash" {
        await bobsVault()

        let vault = await rooms.find(name: "vault")

        expect vault?.passwordHash?.startsWith("$argon2id$") == true
    }

    test "joining asks for the password once, and remembers it" {
        await bobsVault()
        await sessions.signIn(userId: AccountId(name: "ana"))
        let screen = render(Room(name: "vault"))
        await screen.settle()

        expect not screen.textField(labeled: "Message").exists

        screen.passwordField(labeled: "Room password").fill("guess")
        screen.button(named: "Join #vault").click()
        await screen.settle()

        expect screen.alert().text == "That isn't the password of #vault."

        screen.passwordField(labeled: "Room password").fill("open sesame")
        screen.button(named: "Join #vault").click()
        await screen.settle()

        expect not screen.passwordField(labeled: "Room password").exists
        expect screen.list(named: "In this room").items().map { item => item.text } == ["ana"]

        screen.unmount()
        let again = render(Room(name: "vault"))
        await again.settle()

        expect again.textField(labeled: "Message").exists
    }

    test "nobody joins without the password" {
        await bobsVault()
        let ana = await peers.signIn(userId: AccountId(name: "ana"))
        let chat: ChatProtocol = ana.client()

        expect throws RoomError.passwordRequired {
            try await chat.room(name: "vault")
        }
        expect throws RoomError.wrongPassword {
            try await chat.unlock(room: "vault", password: "Open Sesame")
        }
    }

    test "wrong passwords are limited to five a minute" {
        await bobsVault()
        let ana = await peers.signIn(userId: AccountId(name: "ana"))
        let chat: ChatProtocol = ana.client()

        for n in 1...5 {
            expect throws RoomError.wrongPassword {
                try await chat.unlock(room: "vault", password: "guess {n}")
            }
        }
        expect throws RoomError.rpcFailure(.rejected(.rateLimited)) {
            try await chat.unlock(room: "vault", password: "open sesame")
        }
    }

    test "too many tries say so on the page" {
        await bobsVault()
        await sessions.signIn(userId: AccountId(name: "ana"))
        let screen = render(Room(name: "vault"))
        await screen.settle()

        for n in 1...6 {
            screen.passwordField(labeled: "Room password").fill("guess {n}")
            screen.button(named: "Join #vault").click()
            await screen.settle()
        }

        expect screen.alert().text == "Too many tries. Wait a minute, then try again."
    }

    test "a room's messages stay with its members" {
        await bobsVault()
        let ana = await peers.signIn(userId: AccountId(name: "ana"))
        let anasChat: ChatProtocol = ana.client()
        let general = try await anasChat.room(name: "general")
        let lobby: LobbyProtocol = ana.client()
        let anasLobby = try await lobby.lobby()

        let bob = await peers.signIn(userId: AccountId(name: "bob"))
        let chat: ChatProtocol = bob.client()
        let vault = try await chat.room(name: "vault")
        try await vault.send(.post(text: "the code is 1234", clientId: Uuid.random()))
        let room = try await chat.room(name: "general")
        try await room.send(.post(text: "hi all", clientId: Uuid.random()))

        // Ana's lobby hears of #general's message, never of #vault's.
        let events = anasLobby.messages
        match await events.next() {
            case .snapshot(let all) {
                expect all.map { r => r.name } == ["general", "help", "random", "vault"]
            }
            default { expect false }
        }
        expect await events.next() == .posted(room: "general", number: 1, author: "bob")
        general.close()
    }
}
