---
status: Early-Draft, Needs-Expansion
---

When `ah` is launched in remote mode, it issues commands to a remote server that carries out the work (the server executes the tasks in the same way they would work in [Local-Mode](Local-Mode.md)).

Refer to [REST-Service/API.md](REST-Service/API.md).

The majority of the subsystems described in [Local-Mode](Local-Mode.md)—database-backed TaskManager implementations, snapshot orchestration, and the task lifecycle executor—also run inside the access-point server. Remote mode therefore reuses the same core crates (`ah-core`, `ah-local-db`, and the TaskExecutor stack) but exposes them over the REST interface so the TUI/WebUI can drive workspaces from another machine.
