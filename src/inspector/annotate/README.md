# Annotate Module

Recognizers that read meaning out of captured HTTP exchanges.

## Why this is separate from capture

`crate::capture::http` records **everything a backend intercepts** and understands
none of it. This directory is where understanding is added, and it is strictly
additive: a recognizer returns an `HttpAnnotation` that rides along with the
exchange.

The split exists because Mokora talks to several games, and their traffic has
nothing in common above HTTP. Capture stays platform- and vendor-agnostic by
construction; anything vendor-shaped lives here, inside an annotation's `data`,
and **never** in `schema::HttpExchange`.

Concretely: adding a recognizer for a new analytics vendor, a new game, or a new
protocol touches this directory and nothing else — not the schema, not the two
capture backends, not the JSONL reader, not the UI. The UI renders any
annotation from its `kind` / `summary` / `data` without knowing what produced it.

## Annotations also describe Mokora's own behaviour

When the proxy declines to intercept something, that decision is recorded as an
annotation on the CONNECT (`mokora_bypass`), and a response we could not attribute
to a request is marked too (`mokora_unpaired`). When the proxy *drops* a request
instead of forwarding it — an Aliyun SLS telemetry beacon, when `block_telemetry`
is on — that too is recorded (`mokora_blocked`), alongside the beacon's own
`sls_beacon` annotation so the timeline still shows what was dropped. A blind spot
that announces itself is not a blind spot — a timeline that silently omits what we
could not see is indistinguishable from one where the game never sent it.

## Adding a recognizer

1. Add a module here that inspects a `RequestView` and returns
   `Option<HttpAnnotation>`.
2. Call it from `annotate_request`.
3. Give it a stable `kind` — the UI groups and filters on that string, and it is
   what a `jq` query over `inspector.jsonl` will select on.

Keep it cheap. `annotate_request` runs on every intercepted request and every
page subresource, the overwhelming majority of which match nothing. The existing
`sls` recognizer costs one `strip_prefix` and one `split_once` in the common case.

`RequestView` is a struct rather than a parameter list so a recognizer that later
needs headers or a body does not force every call site to change.

## Current recognizers

| kind | what it reads |
|---|---|
| `sls_beacon` | Alibaba Cloud SLS web-tracking beacons — the channel through which Mahjong Soul and Riichi City clients report on themselves. See the module docs in `sls.rs` for the wire format and what varies between deployments. |

## Not covered yet

Two known formats we cannot currently decode, both blocked on **capture**, not on
recognition:

- **Unity Analytics** (`cdp.cloud.unity3d.com`) — Riichi City ships it alongside
  SLS. It POSTs, and a cleartext `POST` inside a `CONNECT` tunnel is raw-tunneled
  by hudsucker, which only recognizes `GET ` and a TLS ClientHello when it peeks
  at an upgraded tunnel.
- **Riichi City's SLS beacons** — its client rejects our MITM certificate for the
  telemetry host, so they never reach us at all.

A recognizer for either would be straightforward; getting the bytes is the hard
part.
