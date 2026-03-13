# capy_net

Networking and multiplayer infrastructure.

## Scope

- Client/server connection management.
- Packet serialization and transport (UDP/TCP).
- State synchronization and replication.
- Lobby, matchmaking, and session management.
- Network clock and latency compensation.

## What Does NOT Belong Here

- Game-specific netcode (what to sync, game rules validation — belongs in `capy_game`).
- HTTP requests for leaderboards or analytics (belongs in `capy_game` or a dedicated service crate).
