import k.rtc.SignallingRelay

/// The rooms' relay.
@Scope(.server)
public service LabRooms {
    let relay: SignallingRelay<String> = SignallingRelay(peersPerRoom: 2)
}
