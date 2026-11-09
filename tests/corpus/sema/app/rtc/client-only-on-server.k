// A peer connection, a sender and a data channel exist only in the browser: naming one in a
// server-only declaration is K4033 (46-media-and-rtc.md, "Server and client").
import k.rtc.DataChannel
import k.rtc.MediaSender
import k.rtc.PeerConnection

struct Line implements Codable {
    let text: String
}

@Scope(.server)
service Calls {
    var current: PeerConnection? = nil

    func hangUp(sender: MediaSender) {
        sender.stop()
    }

    func open(channel: DataChannel<Line>) {
        channel.close()
    }
}
