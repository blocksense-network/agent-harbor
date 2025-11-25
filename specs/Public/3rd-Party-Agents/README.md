# Third-Party Agents — Media & Context Handling (Draft)

> This document describes the tasks and open questions around delivering non-text inputs (images, audio, etc.) from ACP clients to the third-party agents we host inside Agent Harbor. It complements the per-agent guides inside this folder and the recorder/command-tracing specs referenced throughout `specs/ACP.server.status.md`.

## 1. Goals

- Understand how ACP clients send multimodal inputs (images, audio, binary attachments) and how we can forward those payloads to the third-party agent processes we host.
- Identify gaps in each third-party agent’s CLI/protocol and document required translation layers (e.g., converting ACP `image_url` blocks into files on disk, piping audio bytes into a shell command, etc.).
- Provide actionable guidance for the team (and external contributors) when adding a new agent or extending an existing one with multimodal capabilities.

## 2. ACP Background

The ACP spec (`resources/acp-specs/docs`) allows clients to include multimedia content in several places:

- `session/prompt` content blocks can have `type: "image"`, `type: "audio"`, or arbitrary binary attachments (inlined or referenced via URLs).
- Chat messages may include attachments that need to be staged on disk before the agent can consume them.
- Tool calls can return `diff`, `terminal`, or custom content; we may want to associate results with the original media input for auditing.

Therefore, Agent Harbor needs a consistent way to:

1. Accept these inputs over ACP/REST.
2. Store/stage them inside the sandbox (e.g., under `/tmp/ah-media/<session>/<uuid>.<ext>`).
3. Pass references/paths to the third-party agent in whatever format it expects.
4. Clean up artifacts when sessions end or when policies require.

## 3. Research Tasks

1. **Protocol deep-dive**  
   - Study `resources/acp-specs/docs/protocol/content.mdx` and related sections (initialization, prompt-turns) to catalog every media/content type the client might send.
   - Note size limits, streaming modes, and whether ACP clients expect push/pull semantics for attachments.

2. **Per-agent capabilities audit**  
   For each agent supported under `specs/Public/3rd-Party-Agents/`:
   - Document whether the agent supports images/audio today and, if so, how (CLI flags, environment variables, HTTP uploads, etc.).
   - If unsupported, identify what would be required (e.g., implementing `multipart/form-data` upload to a cloud API or staging files in a particular directory).
   - Record any sandboxes/perms needed (e.g., converting audio may require `ffmpeg` inside the sandbox).

3. **Staging & lifecycle design**  
   - Propose a standard layout for temporary media files under the session workspace (`/workspace/.ah-media/…` or similar).
   - Define metadata we track per attachment (source hash, MIME type, size, ACL) for auditing.
   - Decide how long artifacts persist (per session, per snapshot) and how they integrate with time-travel (e.g., snapshots should capture corresponding media files).

4. **Streaming pathway investigation**  
   - Determine whether ACP clients ever stream audio/video (e.g., microphone capture). If yes, identify whether we need a passthrough TTY-like channel, or if staging to disk is sufficient.
   - Map these flows onto the command-execution tracing/recorder stack so recorded sessions include the context needed to reproduce the agent behavior.

5. **Security & policy considerations**  
   - Evaluate size limits, MIME validation, virus scanning requirements, and tenant policies for media uploads.
   - Document how redaction or encryption should work if sensitive media is present.

6. **Client UX alignment**  
   - Survey target IDEs (VS Code, Cursor, WebUI) to understand how they display/collect media inputs today. Capture screenshots or behavior summaries where possible.
   - Ensure our REST/API responses surface enough metadata for IDEs to correlate attachments with Harbor’s staging paths (e.g., provide a `harborMediaId` they can reference later).

## 4. Deliverables

- Updated per-agent specs in `specs/Public/3rd-Party-Agents/` detailing how each agent handles media inputs (or why it doesn’t).
- New sections in `specs/Public/REST-Service/API.md` / `specs/ACP.extensions.md` describing the REST/ACP endpoints used to receive and manage attachments.
- Implementation plan (Milestones in `specs/ACP.server.status.md`) capturing:
  - Media staging subsystem
  - Agent-specific translation layers
  - Recorder + snapshot integration (ensuring media files map to snapshots/time-travel)
  - LLM API Proxy scenarios covering multimodal inputs

## 5. Open Questions

- Should media be versioned/snapshotted alongside code? If so, how do we deduplicate large files across snapshots?
- Do any agents require streaming audio in real time (e.g., voice coding assistants), and can our sandbox deliver low-latency audio channels?
- How do we sanitize/validate media before passing it to the agent (especially for closed-source agents with security requirements)?

## 6. Next Steps

1. Assign owners for each research task above (ACP protocol review, per-agent audits, staging design, etc.).
2. Track findings in this document (sections per agent/content type).
3. Once the research solidifies, translate outcomes into actionable tickets/milestones in `specs/ACP.server.status.md` and associated implementation specs.

