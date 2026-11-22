# Call

A 1:1 video call written in Kobra: sign in with a name, start a call, and send its link to the person you want to talk to. They open it in their browser, and you see and hear each other, with mute, camera off, switching cameras, screen sharing, a text chat and the connection's statistics.

It is a teaching example for `k.rtc` ([`docs/design/46-media-and-rtc.md`](../../docs/design/46-media-and-rtc.md)): calls between two browsers, signalled over a channel of the app's own protocol, with no third-party server anywhere. Two tabs on one machine, or two laptops on one network, connect with no STUN or TURN server at all; a deployment that must connect across networks runs its own coturn next to the app.

## Running it

| Command, at `examples/call` | What it does |
|---|---|
| `k dev` | Builds and serves the app on `http://localhost:8080`. Open it in two tabs (or a normal and a private window), sign in with two names, press **Start a call** in one, and open the call's link in the other. Capture works on `localhost` and over `https` only. |
| `k test` | Runs the tests against `TestPeerConnections` and `TestMediaDevices`: a second person joins through the app's own protocol and relay, and no camera opens. |
| `k check` | Checks the code, runs the tests, and checks `rpc.lock` against the protocols. |
| `k build` | Builds `dist/`, with a `dist/app` that serves the pages, the rpcs and the signalling channel. |
| `docker compose up --build --wait` | Runs it with coturn, as in production. See [Deploying it](#deploying-it). |

There is no database: a room is an unguessable link (a random UUID), and sessions live in the server's memory, so restarting the server signs everyone out.

## What it demonstrates

| Feature | Where |
|---|---|
| A **signalling channel**: `rpc join(room:) throws CallError -> Channel<Signal, Signal, RpcOnlyError>`, guarded by `Authenticated` | `src/calls/CallProtocol.k` |
| A **`SignallingRelay`** that pairs the two people in a room, refusing a third (`CallError.roomFull`) | `src/calls/CallRooms.k`, `src/calls/CallApi.k` |
| **ICE servers** from `IceServers`, with TURN credentials signed by a server-only secret; none in development | `src/calls/CallApi.k`, `compose.yaml` |
| `PeerConnections.connect` with the whole negotiation in the stdlib, and a status line from `PeerConnection.state` | `src/calls/CallService.k`, `src/calls/CallPanel.k` |
| Mute and camera off with `MediaTrack.setEnabled`; switching cameras and sharing the screen with `MediaSender.replace` | `src/calls/CallService.k` |
| **Typed data channels**: `DataChannelKind<ChatLine>` for the chat, `DataChannelKind<Introduction>` for names, merged with `Stream.merge` | `src/calls/chatLines.k`, `src/calls/introductions.k`, `src/calls/CallService.k` |
| The connection's statistics (`stats()`): round trip, direct or relayed | `src/calls/CallPanel.k` |
| Every permission state: a Join button, or how to unblock the camera | `src/calls/CallPanel.k` |
| Two-person tests with `TestPeerConnections` and `TestPeers`: chat, mute, a lost and a dead network, hanging up, screen sharing, a full room | `src/calls/Room.test.k` |

## How it works

### Joining

1. `Lobby` creates a room (`createRoom`, a random UUID) and opens `/room/<id>`.
2. The **Join call** button runs `CallService.join(room:)` from its click, so it may ask for the camera and the microphone (`requestAccess`, the only prompt), then captures them with `lifetime: .service`: they stay on across re-renders and turn off when the page is left.
3. It fetches the ICE servers (`[]` without `K_TURN_URLS`) and opens the room's signalling channel. On the server, `CallApi.join(room:)` joins the relay; a third person gets `roomFull`.
4. `peers.connect(signalling:iceServers:)` waits until the other person joins, then returns the `PeerConnection`. The stdlib exchanges the descriptions and network candidates over the channel, with "perfect negotiation", so offers that cross are resolved; the app never sees one.
5. The service sends the camera and the microphone, introduces itself on the `introductions` data channel, and follows the chat.

### During the call

- **Mute** disables the microphone's track: the other side receives silence, and the device stays open, so unmuting is instant.
- **Switch camera** captures the next camera and gives it to the call with `replace`, which swaps the tracks without renegotiating, then stops the old one.
- **Share screen** captures a screen, window or tab and `replace`s the camera with it; the microphone keeps sending. The browser's own "Stop sharing" ends the screen's track, and the panel goes back to the camera.
- A changed network shows **Reconnecting…** while the stdlib restarts ICE through the same channel; after 15 seconds without recovery the call ends.
- **Hang up** closes the call: the other side sees "The other person hung up."

## Deploying it

`compose.yaml` runs the app and **coturn**, which serves STUN and TURN on one port and relays a call when the browsers can't reach each other directly. Both read one secret: the app signs short-lived TURN credentials with it (coturn's TURN REST scheme, `--use-auth-secret`), and coturn checks them. The browser only ever gets credentials that expire in an hour.

```sh
cp secrets/turn_secret.txt.example secrets/turn_secret.txt   # then put a long random value in it
docker compose up --build --wait
```

Before you deploy, replace `call.example.org` with the host browsers reach and `203.0.113.10` with its public address, and open 8080/tcp (or put the app behind your TLS proxy), 3478/udp, 3478/tcp and 49160–49200/udp, the relay range: about 20 concurrent relayed calls.

**The denied ranges matter.** coturn's `--denied-peer-ip` flags for `10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16` and `127.0.0.0/8`, and `--no-multicast-peers`, stop it relaying into the deployment's own network; without them a TURN server is a proxy into the data centre for anyone with a credential. A deployment test (`crates/k_cli/tests/call_deploy.rs`) checks that `dist/app config`, `compose.yaml` and coturn's flags agree: the secret is a file both mount, the URLs name coturn's port and realm over UDP and TCP, and the private ranges are denied.

`K_COMPOSE=1 cargo test -p k_cli --test it -- compose::call` runs `compose.yaml` itself, with a random secret in place of the template's: the page loads, `iceServers` names coturn with a signed credential, and coturn answers a STUN binding request ([testing.md §8.6](../../docs/compiler/testing.md#86-compose-smoke-tests)). It runs coturn on the project's network with its port published instead of `network_mode: host`, which Docker Desktop on a laptop doesn't provide by default and which would put coturn on 3478 rather than a test port; everything else is this file.

| Variable | Meaning |
|---|---|
| `K_PUBLIC_ORIGIN` | The app's origin; the signalling channel's WebSocket checks it. |
| `K_TURN_URLS` | The `turn:` (and `stun:`, `turns:`) URLs browsers get, comma-separated. Unset: none. |
| `K_TURN_SECRET` or `K_TURN_SECRET_FILE` | coturn's `static-auth-secret`. URLs without it stop the server at startup. |
