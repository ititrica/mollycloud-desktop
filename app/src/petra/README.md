# Petra integration boundary

MollyCloud follows Petra's separation of Live2D rendering, autonomous pet
behavior, and assistant UI. The integration is pinned to Petra commit
`9b4af14efc14696e4b249ecde1ad66008af502e0` (version 0.2.3).

Included in the Molly adaptation:

- model-independent autonomous blink and gesture scheduling;
- desktop speech bubbles driven by assistant and account events;
- OpenAI-compatible assistant flow with tool calls.

Deliberately excluded:

- shell execution, application launching, shutdown, file deletion and trash;
- foreground-window inspection and proactive context collection;
- automatic purchasing, subscription changes, or API-key mutation.

Molly account tools execute in Rust and return allowlisted, read-only fields.
Credentials never cross into the WebView or model context.
