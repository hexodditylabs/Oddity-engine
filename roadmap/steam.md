# Steam

*Decided 14 October 2026; built in Phase 4, with Steam networking in Phase 5
([multiplayer.md](multiplayer.md)).*

## Decisions

| Question | Decision |
| --- | --- |
| Library | **`steamworks`** 0.13 ([steamworks-rs](https://github.com/Noxime/steamworks-rs)). It doesn't depend on Bevy, so it works with Bevy 0.20. |
| Bevy glue | **Our own `oddity_steam` plugin.** `bevy_steamworks` still needs Bevy 0.19, and the glue is small: start Steam, run its callbacks every frame, turn its results into signals. |
| Opt-in | Per project, in Project Settings › Steam: on/off and the App ID. A game without Steam doesn't link it. |
| Without Steam running | The game still runs; `ctx.steam()` returns `None`. |
| Development | App ID 480 (Valve's test app, "Spacewar") until you have your own. Playing from the editor writes `steam_appid.txt` next to the game program, as Steam requires. |

## What it looks like

```rust
#[on(EnemyKilled)]
fn count_kill(&mut self, ctx: &mut Ctx, _: &EnemyKilled) {
    if let Some(steam) = ctx.steam() {
        steam.add_stat("goblins_killed", 1);
        if steam.stat("goblins_killed") >= 100 {
            steam.unlock_achievement("GOBLIN_SLAYER");
        }
    }
}

fn _onready(&mut self, ctx: &mut Ctx) {
    if let Some(steam) = ctx.steam() {
        steam.set_rich_presence("status", "Fighting goblins");
        info!("Hello, {}", steam.player_name());
    }
}
```

Steam answers some requests later (leaderboards, lobbies), so those results
arrive as signals:

```rust
ctx.steam_leaderboard("high_scores").upload(score);

#[on(LeaderboardUploaded)]
fn show_rank(&mut self, ctx: &mut Ctx, done: &LeaderboardUploaded) {
    info!("You're #{}", done.global_rank);
}
```

## What's covered

| Feature | When |
| --- | --- |
| Achievements and stats | S1 |
| Rich presence, player name and avatar, friends | S1 |
| The Steam overlay (open it, invite friends) | S2 |
| Lobbies and invites | S2, used by multiplayer |
| Networking over Steam's relay | Phase 5 ([multiplayer.md](multiplayer.md) N6) |
| Leaderboards | S3 |
| Cloud saves (Steam Remote Storage), through the save system | S3 |
| DLC ownership checks | S3 |
| Exporting with Steam's libraries included | S4 |
| Uploading builds to Steam (SteamPipe) | Later |
| Workshop (user content), Steam Input | Later |

## How it works

- `oddity_steam` starts Steam at launch when the project enables it, runs
  Steam's callbacks once per frame, and exposes `ctx.steam()` (scripts) and a
  `Steam` resource (systems). It's behind the `steam` Cargo feature on
  `oddity`, so a game without Steam doesn't compile or link it.
- `oddity_steam_editor` adds the Project Settings page through
  `oddity_plugin_api`, and stays hidden in projects without the `steam`
  feature (see [modularity.md](modularity.md)).
- Achievement and stat names are defined on Valve's Steamworks website, not in
  Oddity. The editor's Project Settings page shows the App ID and whether Steam
  is reachable.
- Exporting with Steam enabled copies Steam's library (`steam_api64.dll`,
  `libsteam_api.so` or `libsteam_api.dylib`) next to the game, and leaves out
  `steam_appid.txt`. Steam's libraries are under Valve's SDK licence; the
  export adds them to the game's third-party notices.

## Build plan

- [ ] **S1. `oddity_steam`.** Start-up, callbacks, `ctx.steam()`, the
  Project Settings page (in `oddity_steam_editor`; needs modularity P5),
  achievements, stats, rich presence, player info.
  *Done when:* with Steam running and App ID 480, a test game unlocks an
  achievement and shows the player's name.
- [ ] **S2. Overlay, lobbies and invites.**
- [ ] **S3. Leaderboards, cloud saves, DLC checks.**
- [ ] **S4. Steam in exports.**
