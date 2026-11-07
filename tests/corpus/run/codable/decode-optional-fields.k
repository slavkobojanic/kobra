// An optional field decodes from null or from an absent key as nil.
import k.encoding.Json
import k.http.DecodingError

struct Profile implements Codable {
    let name: String
    let bio: String?
}

func main() throws DecodingError -> List<Profile> {
    let explicit: Profile = try Json.decode("{{\"name\": \"a\", \"bio\": null}")
    let absent: Profile = try Json.decode("{{\"name\": \"b\"}")
    let present: Profile = try Json.decode("{{\"name\": \"c\", \"bio\": \"hi\"}")
    return [explicit, absent, present]
}
