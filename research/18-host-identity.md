# 18 · Knowing it's really your computer

Work brief, 2026-11-19. The implementation and verification results are in [the proof report](../tools/identity-test/REPORT.md).

## The problem

The deployed protocol-8 host makes a new self-signed certificate every time it starts (`crates/transport/src/lib.rs:137`), the client accepts any certificate (`connect_insecure`, `crates/client/src/lib.rs:311`, `:1272`), and then sends the computer's key in the clear inside that connection (`Hello`, `Probe`, `UpdateRequest` in `crates/proto/src/messages.rs`). So the client has no way to tell that the endpoint it reached is the computer it meant to reach, and it gives the key to whatever answers. The key is everything: sessions, the login screen in system mode, update requests. Tailscale hides this on a tailnet. Two independent reviews have now rated it high, and it is the one thing that stands between Sunna and being safe to use on a network you don't control. It is also the foundation an optional sign-in would need later: sign-in is a way of distributing exactly the identities defined here.

## What "done" looks like

- In the default mode the key never crosses the wire. The explicit older-computer exception below uses the legacy token.
- With proof required, a different computer at the same address receives nothing that grants access. The app stops before showing its picture.
- A computer added by link or QR code is verified from its very first connection.
- Previously saved computers can learn an identity through proof. Older hosts need a local update or an explicit exception for their saved record.
- If a computer's identity changes, the app says so and stops, like SSH does.

## Design

### 1. Each host has a permanent identity

At setup the host makes a long-lived key pair and certificate, kept beside its key with the same protection (per-user modes: the user's config, 0600; system mode: `/etc/sunna`, root only; macOS: the app's support directory, 0600; Windows: the service's protected store). It is not regenerated at start, and survives updates and migration between modes. Its **identity** is the SHA-256 of the certificate's public key (SubjectPublicKeyInfo). `sunna-host new-identity` replaces it deliberately and says that every device must add the computer again.

### 2. The link and the QR code carry the identity

`sunna://address?key=…&id=<base64url of the 32 bytes>`. Everything that prints or parses a link learns `id`: `scripts/sunna-host`, the Mac host, the app's "Link to this computer" and QR code, `crates/client/src/reach.rs`, the Android parser and store. A link without `id` is still accepted (older hosts).

### 3. The client pins it in the TLS handshake

A machine record with an identity connects through a verifier that accepts exactly one certificate public key. The check is part of the handshake, before the client has said anything. `connect_insecure` remains only for developer tools and tests and is unreachable from the app, the viewer and Android.

### 4. Both sides prove they know the key without sending it

Replace "send the key" with a proof bound to this particular connection:

```
both sides: m = TLS exporter("sunna auth v1", 32 bytes)          // differs for every connection, and on each leg of a relayed one
client → host:   AuthProof  { nonce_c, proof_c = HMAC-SHA256(key, "client" ‖ m ‖ nonce_c) }
host → client:   AuthProofAck { nonce_h, proof_h = HMAC-SHA256(key, "host" ‖ m ‖ nonce_c ‖ nonce_h) }   // only if proof_c verified
```

The client checks `proof_h` before sending anything further and before showing or accepting anything. A relay has a different `m` on each of its two connections, so a forwarded proof does not verify; the key is never transmitted; and only an endpoint that holds the key can produce `proof_h`. This holds even for a record with no pinned identity yet. Constant-time comparison; `ring` already provides HMAC; rustls and quinn expose the exporter. Every place a token is sent today (`Hello`, `Probe`, `UpdateRequest`, anything else found) moves to this. No token field is ever populated by a new client talking to a new host.

A proof is only as strong as the key behind it, so keys must be strong: generated keys are at least 128 bits (they are today: `openssl rand -hex 16`). A key that isn't in the generated format is refused for proof-based authentication with an instruction to run `sunna-host new-key`; research/17 item 1.6 already enforces this for system mode.

### 5. Old and new, without a quiet downgrade

- New clients require proof by default for every record, including the first probe. If an older host answers, send no key and show “Needs an update” with the install-by-hand guidance.
- Only Edit on a never-upgraded record without a pinned identity offers “Allow this older computer”. With that switch on, a viewer may send the old token to an endpoint negotiating the old protocol. Mark that connection “not fully verified”. Anyone impersonating an old host at this address can obtain the key. A successful proof or a link with an identity permanently disables the exception.
- Old client, new host: accepted as today for now, counted, and shown by `sunna-host status` ("an older app connected without full verification"). A host setting `SUNNA_REQUIRE_PROOF=1` refuses them; system mode defaults to requiring it, because of what it exposes.
- A record without an identity that completes a proof exchange stores the identity it saw (trust on first use, now safe to do because the proof showed the host knows the key).
- TLS negotiation offers the proof-capable ALPN first and the deployed ALPN second. An old server selects the latter. The default viewer closes there without opening a control stream. A failed proof on the new ALPN never triggers a second attempt using a token.
- Protocol stays 8; new messages are appended variants and optional tails, as the update work did. Prove old and new on each side.

### 6. When the identity doesn't match

The app refuses to connect and says: "This isn't the computer you added, or it was set up again. If you reinstalled it, add it again with its new link." No "connect anyway". Re-adding by link or QR replaces the identity; Edit shows the identity's short form so two people can compare it by eye. The same state on Android.

## Files

`crates/transport`, `crates/proto`, `crates/client`, `crates/host`, `crates/android`, `apps/sunnad`, `apps/sunna/src` (machine records, link parsing, the QR code, the Mac host's link), `apps/sunna/ui` (the two new states and the Edit line only, in the app's existing look), `apps/android` (parser, store, the same two states), `apps/sunna-cli`, `scripts/sunna-host`, `scripts/system-host.sh`, `docs/security.md`, `docs/using.md`, `docs/troubleshooting.md`.

## Proving it

1. A relay test fixture in the project's own test suite: a small in-process QUIC relay that accepts the client's connection and opens its own to a test host, forwarding messages. With today's code the test shows the defect (the key value appears in the messages the relay handles, and a session is established through it); keep that as the failing baseline. With the new code the same test must show: the key value appears in no message; a forwarded proof does not verify; the relay cannot complete the host's side; and the app shows the identity state. Run it for a pinned record, an unpinned upgraded record, a never-upgraded record with a real old host, and a never-upgraded record with the relay answering like an old host. With the switch off neither old endpoint receives the key. With the switch on the old host connects as not fully verified, and an impersonator receives the key.
2. Identity survives restart, update, rollback, and migration from a per-user host to the system host; `new-identity` changes it and every client refuses until re-added.
3. Old app with new host and new app with old host: probe, connect, update request; the "not fully verified" marking appears exactly when it should.
4. Wrong key, wrong identity, a proof reused from an earlier connection, a proof with a truncated or oversized nonce: all refused, with constant-time comparison, and counted by the failed-sign-in limit from research/17.
5. Link and QR round trips on Linux, the Mac code paths (type-check and unit tests), and Android (JVM tests and the Rust parser).
6. The app's and Android's new states captured in every view, at full size, and looked at.
