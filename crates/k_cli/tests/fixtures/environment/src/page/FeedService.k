/// Pages of posts, loaded one at a time.
public service FeedService {
    @State var posts: List<String> = ["Post 1", "Post 2"]
    @State var loads: Int = 0

    @Derived let hasMore: Bool = posts.count < 6

    func loadMore() async {
        loads += 1
        let next = posts.count + 1
        posts.append("Post {next}")
        posts.append("Post {next + 1}")
    }
}
