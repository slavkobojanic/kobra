import k.crypto.Uuid
import k.rtc.SignallingRelay

/// The rooms' signalling, one instance per server process: a relay that
/// pairs the two people who join a room and forwards each one's signals to
/// the other. It is built on `Presence` and `Broadcast`, so with Postgres
/// the two may be on different replicas.
@Scope(.server)
public service CallRooms {
    let relay: SignallingRelay<Uuid> = SignallingRelay(peersPerRoom: 2)
}
