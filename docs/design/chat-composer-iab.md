# Chat Composer IAB

## Intent

Chat Composer is a focused input workspace: the main editor keeps inline context references, pending agent actions temporarily take over the input, and controls stay grouped in the composer card.

## Behavior Model

- Normal state shows one rich input surface. Text, inline file references, pasted images, pasted paths, and long pasted text all flow through the same context model.
- Pending interactions occupy a sibling card above the composer. AskUser, tool consent, and plan approval keep their own fields and actions; they do not replace the composer.
- Blocking pending (`blocking_pending_count > 0`) disables send. Title-update and other non-blocking pending leave the composer usable.
- Running state with an empty composer turns the send action into interrupt. Running state with content still queues a new message.

## Layout

- Conversation column: timeline body, optional pending card, composer card. Pending is not a child of the composer card.
- Pending card: NanaUI `Stack::column` with `surface` / `outline` / `radius`, actions in `Stack::row`.
- Composer toolbar: image previews first, then attachment, permission, plan, and send/interrupt controls. Slash/mention completion stays inside the composer card.
- Compose with `composer_card()` in `apps/desktop/src/runtime_layout.rs` and NanaUI `Stack` presets. Do not introduce a second visual language for the composer.

## Confirmation Notes

- Visible composer behavior stays on the existing native controls and message flow in `apps/desktop`.
- Contract changes are required only when the interaction payload itself changes.
