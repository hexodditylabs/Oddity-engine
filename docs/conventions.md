# Conventions

House rules for the Oddity codebase. Short on purpose.

## Comments and docs

- **Every module starts with a `//!` header** saying what it is for and how it
  fits in. Someone opening the file cold should know where they are.
- **Every public item has a `///` doc comment.** `missing_docs` is a warning
  in the workspace lints.
- **Comments explain why, not what.** The code already says what it does.
  Write down the reason a reader can't see: a Bevy quirk, a Godot behaviour
  being matched, a trade-off, a non-obvious invariant.

  ```rust
  // Bad: restates the code.
  // Set the width to the size.
  node.width = Val::Px(size);

  // Good: says why.
  // ComputedNode is in physical pixels; Node sizes are logical.
  let size = computed.size() * computed.inverse_scale_factor();
  ```
- **No commented-out code, no "TODO: fix later" without a phase or issue.**
  If something is deferred, say where it lands: "arrives in Phase 3".
- **No code kept "for later".** Unused items are deleted, not silenced with
  `#[allow(unused)]`.

## Code

- `cargo fmt` and `cargo clippy --workspace --all-targets -- -D warnings`
  must pass (CI enforces both).
- Errors are typed (`thiserror`) in library crates; the editor logs them
  with `warn!` where the user can't act on them.
- Level changes go through commands (`docs/architecture.md`). A system that
  mutates level entities directly is a bug.
- Colours come from `ActivePalette`; sizes that recur become named
  constants with a doc comment saying where the number comes from
  ("Godot uses 22", "jackdaw uses 36").

## Tests

- Library crates test their logic directly: `oddity_core` runs headless and
  its tests take about a second; keep it that way.
- Test behaviour, not implementation: "undo restores the deleted children",
  not "the Disabled component is removed".

## Third-party material

- New icons, fonts or code from elsewhere go in `third_party/` (or next to
  their crate) with their licence file, and get a line in
  `THIRD_PARTY_NOTICES.md`.
- Copying a design (sizes, layout) is fine; copying code needs its licence.
