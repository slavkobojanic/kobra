/// A message the fake SMTP server accepted (`GET /_fake/mail`).
public struct ReceivedMail implements Codable {
    let from: String
    let to: List<String>
    let subject: String
    let data: String
}
