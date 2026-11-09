/// An invitation to join a team, with the link that accepts it.
public struct Invite implements Equatable {
    let team: String
    let url: Url

    static var sample: Invite {
        return Invite(team: "Ops", url: Url("https://example.com/join/42"))
    }
}
