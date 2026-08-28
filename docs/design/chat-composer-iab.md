# Chat Composer IAB

## Intent

Chat Composer is a focused input workspace: the main editor keeps inline context references, pending agent actions temporarily take over the input, and controls stay grouped in the composer card.

## Behavior Model

- Normal state shows one rich input surface. Text, inline file references, pasted images, pasted paths, and long pasted text all flow through the same context model.
- Pending state takes over the input surface. AskUser, tool consent, and plan approval continue to use the composer entry area for answers, rejection notes, or modification requests.
- Running state with an empty composer turns the send action into interrupt. Running state with content still queues a new message.

## Layout

- Stage: pending panel or rich input, followed by the context search panel when active.
- Toolbar: image previews first, then attachment, permission, plan, and send/interrupt controls.
- Compose with `HostStack::composer_card()` in `apps/desktop/src/runtime_layout.rs` and NanaUI `Stack` presets. Do not introduce a second visual language for the composer.

## Confirmation Notes

- Visible composer behavior stays on the existing native controls and message flow in `apps/desktop`.
- Contract changes are required only when the interaction payload itself changes.
