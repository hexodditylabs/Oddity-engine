# Multiplayer

*Decided 14 October 2026; built in Phase 5. Builds on
[scripting.md](scripting.md): read that first. The tasks at the end are meant
to be done one at a time, in order.*

## Decisions

| Question | Decision |
| --- | --- |
| Model | **Server-authoritative**, like Unity's Netcode for GameObjects. One player hosts (host = server + player) or a dedicated server runs headless. |
| Library | **`bevy_replicon`** 0.45, which supports Bevy 0.20. It replicates components and sends messages; how bytes travel is up to a transport. |
| Transports | **QUIC** (`bevy_replicon_quinnet` 0.21, supports Bevy 0.20) for direct IP, LAN and dedicated servers. **Steam** (our own transport on `steamworks` 0.13) for Steam lobbies, invites and Valve's relay, which gets through home routers without port forwarding. |
| What syncs | Nodes marked **Networked** appear on every machine. Script fields marked `#[sync]` follow the server's value. A **NetworkTransform** keeps movement in sync, smoothed on clients. |
| Messages | **RPCs**: `#[rpc(server)]` methods run on the server when a client calls them; `#[rpc(clients)]` methods run on every client. |
| Ownership | Every networked node has an owner: the server, or one client (that client's player, say). |
| Testing | **Play with several players** from the editor: the host and the clients run as separate windows on your machine, already connected. |
| Prediction | First version: server-authoritative with smoothing, and owner-moved characters. Fine for co-op and most games. Client-side prediction and rollback (for fast competitive shooters) come later. |

## What it looks like

The level gets a **NetworkManager** node (Unity has the same), which hosts,
joins and spawns a player for everyone who connects:

```
Main
├── NetworkManager     player_prefab = player.prefab, max_players = 4
├── Camera             [CameraFollow]
└── Enemies            Networked
    └── Goblin         Networked, NetworkTransform   [Enemy] [Health]
```

`player.prefab` is marked **Networked** and has a **NetworkTransform** with
*authority: Owner*, so each player moves their own character and everyone
else sees it move:

```rust
use oddity::prelude::*;

#[script(id = "4f9d2c71")]
pub struct Player {
    #[default(6.0)]
    pub speed: f32,
    pub bullet: Prefab,
    pub shoot_sound: AudioClip,
    #[sync]
    pub display_name: String,          // set by the server, seen by everyone
}

impl Script for Player {
    fn _onprocess(&mut self, ctx: &mut Ctx, delta: f32) {
        // Every machine has every player; only the owner reads input.
        if !ctx.is_owner() {
            return;
        }
        let movement = ctx.input().axis2("move");
        ctx.translate(Vec3::new(movement.x, 0.0, -movement.y) * self.speed * delta);

        if ctx.input().action_just_pressed("fire") {
            let aim = ctx.forward();
            rpc!(ctx, Player::fire, aim);          // ask the server to shoot
        }
    }
}

impl Player {
    /// Runs on the server. The server decides; clients only ask.
    #[rpc(server)]
    fn fire(&mut self, ctx: &mut Ctx, aim: Vec3) {
        let at = Transform::from_translation(ctx.position()).looking_to(aim, Vec3::Y);
        ctx.instantiate(&self.bullet, at);         // a networked prefab: appears everywhere
        rpc!(ctx, Player::shot_effects);
    }

    /// Runs on every client: effects only, no game state.
    #[rpc(clients)]
    fn shot_effects(&mut self, ctx: &mut Ctx) {
        ctx.play_sound(&self.shoot_sound);
    }
}
```

Health from the shooter example only needs one change, so every client shows
the same health bar: `#[sync] current: f32`.

Hosting and joining, from a menu script:

```rust
ctx.network().host(7777);                                   // QUIC, direct IP or LAN
ctx.network().join("192.168.1.20:7777");
ctx.network().host_steam_lobby(LobbyKind::FriendsOnly, 4);  // Steam: invites, no port forwarding
ctx.network().join_steam_lobby(lobby);
ctx.network().leave();
```

And reacting to people coming and going:

```rust
#[on(PlayerJoined)]
fn welcome(&mut self, ctx: &mut Ctx, joined: &PlayerJoined) {
    info!("{} joined", joined.name);
}
```

The other signals are `PlayerLeft`, `ConnectedToServer`, `Disconnected` and
`ConnectionFailed`.

## Reference

| | |
| --- | --- |
| **Networked** (node setting in the Inspector) | The node and its children exist on every machine |
| **NetworkTransform** (component) | Syncs position/rotation/scale; `authority: Server` or `Owner`; smoothing on clients |
| `#[sync]` field | Follows the server's value. Only the server (or the owner, for owner-authority nodes) changes it |
| `#[rpc(server)]` method | Called by a client, runs on the server. Arguments must be serializable |
| `#[rpc(clients)]` method | Called by the server, runs on every client |
| `#[rpc(owner)]` method | Called by the server, runs only on the owning client |
| `rpc!(ctx, Script::method, args..)` | Call an RPC |
| `ctx.is_server()`, `ctx.is_client()`, `ctx.is_host()` | Where this code is running |
| `ctx.is_owner()`, `ctx.owner()` | Who owns this node |
| `ctx.local_player()` | This machine's player id |
| `ctx.network()` | `host`, `join`, `host_steam_lobby`, `join_steam_lobby`, `leave`, `players()` |
| Signals | `PlayerJoined`, `PlayerLeft`, `ConnectedToServer`, `Disconnected`, `ConnectionFailed` |

A game with no NetworkManager is single-player, and none of this costs anything.

## How it works

- **Replication.** Networked nodes get replicon's `Replicated` marker. A
  networked prefab is sent as its asset ID; clients spawn it from their own
  copy of the prefab, then receive the synced values. `NodeRef`s are mapped to
  each machine's entities (replicon maps entities, as our level loader does).
- **`#[sync]` fields.** For a script with synced fields, `#[script]` also
  generates a small component holding just those fields, registered with
  replicon. The runner copies the fields into it on the server after each
  stage, and back into the script on clients before each stage.
- **RPCs.** `#[rpc]` generates a message type per method (node, arguments)
  registered as a replicon client or server event. The runner delivers them in
  `Update` before `_onprocess`, through the usual take-out/put-back.
- **Transports.** QUIC through `bevy_replicon_quinnet`. Steam through
  `oddity_net_steam`: a replicon backend that moves replicon's messages over
  Steam's networking API, with lobbies from Steam matchmaking.
- **Play with several players.** The Play button gets a player count. With
  more than one, the editor starts the host plus clients as separate
  processes on localhost over QUIC, tiles their windows and gives each its own
  Output tab. The live link attaches to the host.
- **Dedicated servers.** An export target that builds the game without
  window, rendering or audio (`--headless`).

## Crates

| Crate | Layer | What |
| --- | --- | --- |
| `oddity_net` | runtime | NetworkManager, Networked, NetworkTransform, `#[sync]` and RPC support, sessions, signals, the transport interface |
| `oddity_net_quic` | runtime, backend | The QUIC transport (wraps `bevy_replicon_quinnet`) |
| `oddity_net_steam` | runtime, backend | The Steam transport and lobbies (on `oddity_steam`) |
| `oddity_net_editor` | extension | Network nodes in Add Node, the Networked setting in the Inspector, the player count on Play |

The `#[sync]` and `#[rpc]` attributes live in `oddity_script_macros`.
Transports are back ends: games pick them with the `net-quic` and `net-steam`
Cargo features on `oddity`, and game code reaches them only through
`ctx.network()`, never by importing a transport crate (see
[modularity.md](modularity.md)).

## Build plan

Needs scripting milestones A to E done; N6 also needs [Steam](steam.md) S1.

- [ ] **N1. Sessions over QUIC.** `oddity_net`, `oddity_net_quic`, the
  NetworkManager node, `host`/`join`/`leave`, the connection signals.
  *Done when:* two headless test apps connect, and `PlayerJoined` fires on the host.
- [ ] **N2. Networked nodes.** The Networked setting, prefab spawning by asset
  ID, NetworkTransform with smoothing and both authorities, player spawning.
  *Done when:* a goblin spawned on the host appears and moves on a client.
- [ ] **N3. `#[sync]` fields.** *Done when:* the host damages a goblin and the
  client's Health shows the same value.
- [ ] **N4. RPCs.** `#[rpc(server|clients|owner)]` and `rpc!`.
  *Done when:* the Player example above works between two test apps.
- [ ] **N5. Play with several players.** The player count on Play, tiled
  windows, an Output tab per process.
- [ ] **N6. Steam transport and lobbies.** `oddity_net_steam`; host, invite
  and join through Steam.
- [ ] **N7. Dedicated server export.**

## Later

- Client-side prediction and rollback, for fast competitive games. Lightyear
  is the main Bevy library for this; it needs Bevy 0.19 today, so we'd revisit
  once it supports 0.20, or build on replicon's extensions.
- Interest management: only send what each player can see.
- Lag compensation for hit detection; voice chat.
