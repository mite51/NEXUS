# NEXUS CLI - Neo's Setup Notes

## Environment

- **Rust toolchain:** `~/.cargo/bin` (NOT in default PATH — prepend with `export PATH="$HOME/.cargo/bin:$PATH"`)
- **Binary location:** `/home/horde/.openclaw/workspace/NEXUS/target/release/nexus`
- **Source:** `/home/horde/.openclaw/workspace/NEXUS/`
- **Build:** `cargo build -p nexus-cli --release`
- **Tests:** `cargo test --workspace` (137 tests as of 2026-05-25)

## My Identity (Neo's ephemeral test node)

- **Vault:** `/tmp/nexus-neo/vault.json`
- **Passphrase:** `testpass`
- **DID:** `did:nexus:94UziBHERjPeifNGnRM3xcaidr9KmBzuD9Bu2A3YYmVV`
- **Store dir:** `/tmp/nexus-neo/.nexus-store`

⚠️ This identity lives in `/tmp` — it may not survive reboots. Recreate if needed:
```bash
mkdir -p /tmp/nexus-neo && cd /tmp/nexus-neo
printf 'testpass\ntestpass\n' | nexus init --vault vault.json
```

## Jason's Node Info

- **DID:** `did:nexus:AXHMqjZhu1G5RrLw8ghu6E4fwFLLE5TCDvSgBt3NmUHN`
- **PeerID:** `12D3KooWKLeXas9R5uXZqjrMmHTEs29WRaNyFmXgBWohZsCGfR1J`
- **Relay PeerID:** `12D3KooWDFaYV9rwnkAr72CYceoUzhLEoafTXB9MxRw4E7cD4rAv`
- **Public IP:** `75.156.22.206`
- **Relay port:** TCP 4002
- **PRE Public Key:** `03e1cf049c79f9d3e580a782cca8e627157e92c56fb7a24f06c9b90fd2a2ea998e`

### Contact JSON (for join-request flow)
```json
{"name":"Jason","peer_id":"12D3KooWKLeXas9R5uXZqjrMmHTEs29WRaNyFmXgBWohZsCGfR1J","pre_public_key_hex":"03e1cf049c79f9d3e580a782cca8e627157e92c56fb7a24f06c9b90fd2a2ea998e"}
```

## Push Command Usage

```bash
nexus push <FILE> \
  --peer <TARGET_PEER_ID> \
  --folder "/" \
  --relay "/ip4/75.156.22.206/tcp/4002/p2p/12D3KooWDFaYV9rwnkAr72CYceoUzhLEoafTXB9MxRw4E7cD4rAv" \
  --vault vault.json
```

### Push to Jason:
```bash
cd /tmp/nexus-neo
printf 'testpass\n' | nexus push test-push.txt \
  --peer "12D3KooWKLeXas9R5uXZqjrMmHTEs29WRaNyFmXgBWohZsCGfR1J" \
  --folder "/" \
  --relay "/ip4/75.156.22.206/tcp/4002/p2p/12D3KooWDFaYV9rwnkAr72CYceoUzhLEoafTXB9MxRw4E7cD4rAv" \
  --vault vault.json
```

## Known Issues / Gotchas

1. **Relay connection**: First attempt failed with "Response from behaviour was canceled" — this means the relay or target node wasn't reachable at that moment. Jason needs his relay + node running.

2. **Auth requirement**: For push to succeed, BOTH sides need each other as contacts:
   - **Sender (me):** Needs target as contact ✅ (done)
   - **Receiver (Jason):** Needs sender's DID as contact with at least `read` access AND a folder grant. Otherwise `authorize_push` will deny.

3. **Passphrase prompts**: The CLI reads from stdin. Use `printf 'pass\n' |` or `echo pass |` for non-interactive.

4. **PATH**: Always run `export PATH="$HOME/.cargo/bin:$PATH"` first or use full path to binary.

5. **mDNS noise**: The ephemeral node connects to a Kademlia DHT and discovers random peers. The "Connected to 12D3Koo..." messages for random peers are normal — it's looking for the target peer on the network.

## For Push Test To Work

Jason needs to:
1. Have his relay server running (`nexus relay --port 4002`)
2. Have his node running and connected to the relay
3. Add Neo's DID (`did:nexus:94UziBHERjPeifNGnRM3xcaidr9KmBzuD9Bu2A3YYmVV`) as a contact
4. Grant at least `read` access on folder `/`

## Quick Reference

```bash
# Build
export PATH="$HOME/.cargo/bin:$PATH"
cd /home/horde/.openclaw/workspace/NEXUS
cargo build -p nexus-cli --release

# Test
cargo test --workspace

# Run push (from /tmp/nexus-neo)
printf 'testpass\n' | ./target/release/nexus push FILE --peer PEER --relay RELAY --vault vault.json

# Check identity
printf 'testpass\n' | nexus identity --vault /tmp/nexus-neo/vault.json
```
