# Call: a video call between two browsers

`examples/call` is a 1:1 video call: sign in with a name, start a call, and send its link to the
other person, who opens it in their browser. Both see and hear each other, mute, turn the camera
off, switch cameras, share the screen, chat, and see the connection's statistics. It is the example
of [calls between browsers](../apps/media.md#calls-between-browsers), and needs no account and no
external server: two tabs on one machine connect with no STUN or TURN server at all.

```text
call/
├── App.k, k.toml, rpc.lock, routes.lock
├── compose.yaml, Dockerfile, secrets/turn_secret.txt.example
└── src/
    ├── calls/     CallProtocol, CallApi, CallRooms (the relay), CallService, CallPanel, Lobby, Room and its tests
    ├── accounts/  signing in with a name
    └── shell/     AppLayout, CallTheme
```

## Running it

| Command, at `examples/call` | What it does |
|---|---|
| `k dev` | Builds and serves the app on port 8080. Open `http://localhost:8080` in two tabs or two browsers, sign in with two names, start a call in one and open its link in the other. |
| `k test` | Runs the tests against `TestPeerConnections` and `TestMediaDevices`: a second person joins through the app's own protocol, and no camera opens. |
| `k build` | Builds `dist/`, whose `dist/app` serves the pages, the rpcs and the signalling channel. |
| `docker compose up --build --wait` | Runs it with its own TURN server, coturn, for calls across networks. See [Deploying it](#deploying-it). |

## Joining a call

`CallService` is client-only and route-scoped: it injects `PeerConnections` and `MediaDevices`.
`join(room:)` runs from the Join button's click, so it may ask for the camera and the microphone;
then it captures them, fetches the ICE servers, opens the room's signalling channel and waits for
the other person:

```k
// examples/call/src/calls/CallService.k
    func join(room: Uuid) async {
        problem = nil
        if devices.cameraPermission == .prompt or devices.microphonePermission == .prompt {
            await devices.requestAccess(for: .cameraAndMicrophone) // @UserAction: the only prompt
        }
```

Once connected it sends the camera and the microphone, introduces itself on the `introductions`
data channel, so the other page can say "Sam's camera", and follows the chat. The status line comes
from the connection's `state`: "Waiting for someone to join", "Connecting…", "Reconnecting…", or
why the call ended.

## Screen sharing and switching cameras

Both go through `MediaSender.replace(with:)`, so the call never renegotiates: sharing replaces the
camera's video with the screen and keeps the microphone; the browser's own "Stop sharing" button
ends the screen's track, and `CallPanel`'s effect goes back to the camera:

```k
// examples/call/src/calls/CallService.k
    func shareScreen() async {
        do {
            let shared = try await devices.captureScreen(lifetime: .service)
            screen = shared
            sender?.replace(with: shared)
        } catch .cancelled {
```

## Tests

`src/calls/Room.test.k` has ten tests: two people see each other and chat, muting reaches the other
side, a lost network shows "Reconnecting…" and recovers, a network that doesn't come back ends the
call after 15 seconds of the `TestClock`, hanging up, screen sharing and the browser's Stop sharing,
a full room, blocked devices, the connection's statistics, and no ICE servers without TURN.

The stdlib's own real-browser suite (`crates/k_cli/tests/chrome.rs`) calls between two Chrome tabs
through a Kobra server with no ICE servers, through a coturn container with `IcePolicy.relayOnly`
(checking that `stats()` reports the relay), under forced glare, and with a mid-call `replace`.

## Deploying it

`compose.yaml` runs the app and coturn. coturn serves STUN and TURN on port 3478 with
`--use-auth-secret`, sharing one secret file with the app's `K_TURN_SECRET_FILE`, and denies
relaying to every private range and to loopback: without those flags a TURN server is a proxy into
the deployment's own network. A deployment test (`crates/k_cli/tests/call_deploy.rs`) checks that
`dist/app config`, the Compose file and coturn's flags agree.
