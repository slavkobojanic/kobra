/// The payload of `RoomAlertJob`: the message, as posted. The job pushes
/// its text, so a message deleted before the job runs is still announced;
/// a click opens the room, where it is gone.
struct NewMessageAlert implements Codable, Equatable {
    let room: String
    let author: String
    let text: String
}
