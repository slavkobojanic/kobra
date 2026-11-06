test NoteDraft {
    test "tag labels are trimmed, lower-cased and de-duplicated, in the order written" {
        let labels = draft(title: "Plans", tags: " Work, ideas,,work ").tagLabels()

        expect labels == ["work", "ideas"]
    }

    test "a tag field of only commas and spaces gives no tags" {
        expect draft(title: "Plans", tags: " , ,").tagLabels().isEmpty
    }

    test "a summary's draft round-trips its values, with tags comma-separated" {
        let note = summary(title: "Plans", body: "Step one", tags: ["ideas", "work"])

        expect note.draft() == draft(title: "Plans", body: "Step one", tags: "ideas, work")
        expect note.draft().tagLabels() == note.tags
    }

    test "a summary is searchable by title, text and tags" {
        let note = summary(title: "Plans", body: "Step one", tags: ["work"])

        expect note.matches(query: "PLANS")
        expect note.matches(query: "step")
        expect note.matches(query: "work")
        expect note.matches(query: "holiday") == false
    }
}
