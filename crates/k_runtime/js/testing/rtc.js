/**
 * @module k:testing/rtc
 *
 * The in-process media engine behind `k.test.TestPeerConnections`
 * (docs/design/46-media-and-rtc.md, "Loopback peers"): a stand-in for
 * `RTCPeerConnection` with the browser's signalling states, so
 * `runtime/rtc.js` runs its real perfect negotiation over it, with ICE
 * restarts and glare, while tracks and data channels pass between the
 * stand-ins of one test without a network.
 *
 * A description's `sdp` is JSON naming its connection, its ICE generation
 * and the tracks it sends; a candidate names its connection and
 * generation. Two connections are `connected` once each holds the other's
 * description, both are `stable`, each has a candidate of the other's
 * current generation, and the network is up. A received track is a
 * stand-in whose `$src` is the sender's track (replaced in place by
 * `replaceTrack`); data channels with the same negotiated id pair up once
 * both connections are connected. Events fire a microtask later, as the
 * browser fires them a task later.
 */

/** A stand-in candidate: `candidate:<connection> <generation> typ …`. */
const CANDIDATE = new RegExp("^candidate:(\\d+) (\\d+)");

/**
 * A new engine: `RTCPeerConnection` and the network's switches.
 * @returns {{RTCPeerConnection: Function, lost(): void, restored(): void, iceFails(): void, glare(): void, reset(): void}}
 */
export function fakeRtc() {
  const pcs = new Map();
  let up = true;
  let ids = 0;
  const later = (f) => queueMicrotask(f);
  // Candidates come after the description that produced them, as a
  // browser fires them tasks later: a few microtasks later here.
  const soon = (f, n = 8) => (n ? queueMicrotask(() => soon(f, n - 1)) : f());

  /** A data channel stand-in of `pc` with negotiated id `id`. */
  function channel(pc, id, label) {
    const dc = {
      id,
      label,
      readyState: "connecting",
      bufferedAmount: 0,
      bufferedAmountLowThreshold: 0,
      onopen: null,
      onmessage: null,
      onclose: null,
      addEventListener() {},
      send(data) {
        if (dc.readyState !== "open") throw new Error("InvalidStateError: the data channel isn't open");
        const other = dc.$peer;
        later(() => other?.readyState === "open" && other.onmessage?.({ data }));
      },
      close() {
        if (dc.readyState === "closed") return;
        dc.readyState = "closed";
        pc.channels.delete(id);
        const other = dc.$peer;
        dc.$peer = null;
        // What was sent before closing still arrives first.
        if (other && other.readyState !== "closed") {
          later(() => {
            other.readyState = "closed";
            other.$peer = null;
            other.onclose?.();
          });
        }
      },
      $peer: null,
    };
    return dc;
  }

  class FakePeerConnection {
    constructor(config) {
      this.id = ++ids;
      this.relay = config?.iceTransportPolicy === "relay";
      this.signalingState = "stable";
      this.iceConnectionState = "new";
      this.localDescription = null;
      this.remoteDescription = null;
      this.senders = [];
      this.channels = new Map();
      this.received = new Map();
      this.stream = { onremovetrack: null };
      this.version = 0;
      this.offered = 0;
      this.agreed = 0;
      this.gen = 1;
      this.failedAt = 0;
      this.peer = 0;
      this.peerGen = 0;
      this.heard = 0;
      this.closed = false;
      this.needing = false;
      for (const k of ["onnegotiationneeded", "onicecandidate", "oniceconnectionstatechange", "ontrack"]) this[k] = null;
      pcs.set(this.id, this);
    }

    /** Something changed that the peer must hear of: negotiation is needed once stable. */
    changed() {
      this.version++;
      this.needed();
    }

    /** Fires `negotiationneeded` a microtask later, while stable and behind. */
    needed() {
      if (this.needing) return;
      this.needing = true;
      later(() => {
        this.needing = false;
        if (!this.closed && this.signalingState === "stable" && this.offered < this.version) this.onnegotiationneeded?.();
      });
    }

    addTrack(track) {
      const s = { id: ++ids, track, replaceTrack: async (t) => ((s.track = t), this.other()?.retarget(s)) };
      this.senders.push(s);
      this.changed();
      return s;
    }

    removeTrack(s) {
      this.senders = this.senders.filter((x) => x !== s);
      this.changed();
    }

    getSenders() {
      return [...this.senders];
    }

    createDataChannel(label, o) {
      const dc = channel(this, o.id, label);
      this.channels.set(o.id, dc);
      if (this.channels.size === 1) this.changed();
      later(() => this.link());
      return dc;
    }

    restartIce() {
      this.gen++;
      this.changed();
    }

    /** The connection at the other end, once a description named it. */
    other() {
      return pcs.get(this.peer) ?? null;
    }

    async setLocalDescription(d) {
      await null;
      if (this.closed) throw new Error("InvalidStateError: closed");
      if (d?.type === "rollback") {
        this.signalingState = "stable";
        this.offered = this.agreed;
        return;
      }
      const type = d?.type ?? (this.signalingState === "have-remote-offer" ? "answer" : "offer");
      if (type === "answer" && this.signalingState !== "have-remote-offer") throw new Error("InvalidStateError: no offer to answer");
      this.offered = this.version;
      this.localDescription = { type, sdp: JSON.stringify({ pc: this.id, gen: this.gen, tracks: this.senders.map((s) => s.id) }) };
      this.signalingState = type === "offer" ? "have-local-offer" : "stable";
      const gen = this.gen;
      soon(() => {
        if (this.closed) return;
        this.onicecandidate?.({ candidate: { candidate: `candidate:${this.id} ${gen} typ ${this.relay ? "relay" : "host"}`, sdpMid: "0", sdpMLineIndex: 0 } });
        this.onicecandidate?.({ candidate: null });
      });
      if (this.signalingState === "stable") this.negotiated();
    }

    async setRemoteDescription(d) {
      await null;
      if (this.closed) throw new Error("InvalidStateError: closed");
      const info = JSON.parse(d.sdp);
      if (d.type === "offer") {
        // An offer while offering rolls ours back, as browsers do: what it
        // carried still needs negotiating.
        if (this.signalingState === "have-local-offer") {
          this.signalingState = "stable";
          this.offered = this.agreed;
        }
        this.signalingState = "have-remote-offer";
        // An offer from a restarted peer restarts this end's ICE too.
        if (info.pc === this.peer && info.gen !== this.peerGen) this.gen++;
      } else {
        if (this.signalingState !== "have-local-offer") throw new Error("InvalidStateError: no offer for this answer");
        this.signalingState = "stable";
      }
      this.remoteDescription = d;
      this.peer = info.pc;
      this.peerGen = info.gen;
      this.sync(info.tracks);
      if (this.signalingState === "stable") this.negotiated();
    }

    async addIceCandidate(c) {
      await null;
      if (!this.remoteDescription) throw new Error("InvalidStateError: no remote description");
      const [, pc, gen] = CANDIDATE.exec(c.candidate) ?? [];
      if (+pc === this.peer) this.heard = Math.max(this.heard, +gen);
      this.connect();
    }

    /** Back to stable: renegotiate what changed meanwhile, and connect when both ends can. */
    negotiated() {
      this.agreed = this.offered;
      if (this.offered < this.version) this.needed();
      this.connect();
    }

    /** The tracks the peer's description sends: new ones arrive, missing ones are removed. */
    sync(ids) {
      const o = this.other();
      for (const id of ids) {
        if (this.received.has(id)) continue;
        const s = o?.senders.find((x) => x.id === id);
        if (!s?.track) continue;
        const t = {
          kind: s.track.kind,
          label: s.track.label,
          muted: false,
          $src: s.track,
          get enabled() {
            return t.$src.enabled !== false;
          },
          set enabled(v) {},
          stop() {},
          getSettings: () => ({}),
        };
        this.received.set(id, t);
        later(() => !this.closed && this.ontrack?.({ track: t, streams: [this.stream] }));
      }
      for (const [id, t] of this.received) {
        if (ids.includes(id)) continue;
        this.received.delete(id);
        later(() => this.stream.onremovetrack?.({ track: t }));
      }
    }

    /** `replaceTrack` on the peer's sender `s`: the received stand-in now plays its new track. */
    retarget(s) {
      const t = this.received.get(s.id);
      if (t && s.track) t.$src = s.track;
    }

    /** `true` when this end can reach `o`. */
    ready(o) {
      return this.signalingState === "stable" && this.peer === o.id && this.heard >= o.gen && this.gen > this.failedAt;
    }

    /** Connects this end and its peer when both can reach each other. */
    connect() {
      const o = this.other();
      if (!o || o.closed || this.closed || !up || !this.ready(o) || !o.ready(this)) return;
      this.state("connected");
      o.state("connected");
      this.link();
      o.link();
    }

    /** Sets the ICE state, with its event a microtask later. */
    state(s) {
      if (this.iceConnectionState === s || this.closed) return;
      this.iceConnectionState = s;
      later(() => !this.closed && this.oniceconnectionstatechange?.());
    }

    /** Pairs and opens the data channels both connected ends created. */
    link() {
      const o = this.other();
      if (!o || this.iceConnectionState !== "connected" || o.iceConnectionState !== "connected") return;
      for (const [id, dc] of this.channels) {
        const theirs = o.channels.get(id);
        if (!theirs || dc.$peer === theirs) continue;
        dc.$peer = theirs;
        theirs.$peer = dc;
        for (const x of [dc, theirs]) {
          x.readyState = "open";
          later(() => x.readyState === "open" && x.onopen?.());
        }
      }
    }

    async getStats() {
      await null;
      const connected = this.iceConnectionState === "connected";
      const report = new Map();
      if (connected) {
        report.set("T", { id: "T", type: "transport", selectedCandidatePairId: "P" });
        report.set("P", { id: "P", type: "candidate-pair", state: "succeeded", nominated: true, localCandidateId: "L", currentRoundTripTime: 0.001, availableOutgoingBitrate: 1000000 });
        report.set("L", { id: "L", type: "local-candidate", candidateType: this.relay || this.other()?.relay ? "relay" : "host" });
      }
      for (const [id] of this.received) report.set(`I${id}`, { id: `I${id}`, type: "inbound-rtp", packetsLost: 0 });
      return report;
    }

    close() {
      if (this.closed) return;
      this.closed = true;
      this.iceConnectionState = "closed";
      this.signalingState = "closed";
      pcs.delete(this.id);
      for (const dc of [...this.channels.values()]) dc.close();
      // The peer's media stops arriving: its ICE notices, as a browser's does.
      const o = this.other();
      if (o && o.peer === this.id) o.state("disconnected");
    }
  }

  return {
    RTCPeerConnection: FakePeerConnection,
    /** Every path breaks: connected ends go `disconnected`. */
    lost() {
      up = false;
      for (const pc of pcs.values()) if (pc.iceConnectionState === "connected") pc.state("disconnected");
    },
    /** Paths work again: ends that can reach each other connect. */
    restored() {
      up = true;
      for (const pc of pcs.values()) pc.connect();
    },
    /** ICE fails everywhere; only a restart (a new generation) connects again. */
    iceFails() {
      for (const pc of pcs.values()) {
        pc.failedAt = pc.gen;
        pc.state("failed");
      }
    },
    /** Both ends of every connection need to negotiate in the same moment. */
    glare() {
      for (const pc of pcs.values()) pc.version++;
      for (const pc of pcs.values()) if (pc.signalingState === "stable") pc.onnegotiationneeded?.();
    },
    /** Forgets every connection (the test ended). */
    reset() {
      for (const pc of [...pcs.values()]) pc.close();
      up = true;
    },
  };
}
