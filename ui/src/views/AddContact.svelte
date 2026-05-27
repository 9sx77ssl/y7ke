<script lang="ts">
  import { sendContactRequestAction } from "../lib/stores/requests.svelte";
  import { isValidY7Uri } from "../lib/format";
  import { openEmpty } from "../lib/stores/route.svelte";

  let y7Input = $state("");
  let greeting = $state("");
  let submitting = $state(false);
  let error = $state<string | null>(null);
  let success = $state<string | null>(null);

  const trimmed = $derived(y7Input.trim());
  const valid = $derived(isValidY7Uri(trimmed));

  async function submit(): Promise<void> {
    if (!valid || submitting) return;
    submitting = true;
    error = null;
    success = null;
    try {
      const g = greeting.trim();
      await sendContactRequestAction(trimmed, g.length === 0 ? null : g);
      success = "Request sent.";
      y7Input = "";
      greeting = "";
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      submitting = false;
    }
  }

  function cancel(): void {
    openEmpty();
  }
</script>

<section class="page">
  <header class="head">
    <h1>Add contact</h1>
    <p class="sub">Paste a Y7KE identity to send a contact request.</p>
  </header>

  <form
    onsubmit={(ev) => {
      ev.preventDefault();
      void submit();
    }}
  >
    <label class="field">
      <span>Identity</span>
      <textarea
        bind:value={y7Input}
        placeholder="y7:…"
        rows="2"
        spellcheck="false"
        autocapitalize="off"
        autocomplete="off"
        required
      ></textarea>
      {#if y7Input.trim().length > 0 && !valid}
        <span class="hint warn">
          That doesn't look like a valid <code>y7:</code> identity.
        </span>
      {/if}
    </label>

    <label class="field">
      <span>Greeting <em>(optional)</em></span>
      <textarea
        bind:value={greeting}
        placeholder="Say hi"
        rows="3"
        maxlength="500"
      ></textarea>
    </label>

    <div class="row">
      <button type="submit" disabled={!valid || submitting}>
        {submitting ? "Sending…" : "Send request"}
      </button>
      <button type="button" class="ghost" onclick={cancel} disabled={submitting}>
        Cancel
      </button>
    </div>

    {#if error}
      <p class="msg err">{error}</p>
    {/if}
    {#if success}
      <p class="msg ok">{success}</p>
    {/if}
  </form>
</section>

<style>
  .page {
    height: 100%;
    overflow-y: auto;
    padding: 2rem 2.5rem;
  }
  .head {
    margin-bottom: 1.5rem;
  }
  h1 {
    margin: 0;
    font-size: 1.25rem;
    font-weight: 600;
    letter-spacing: -0.01em;
  }
  .sub {
    margin: 0.25rem 0 0;
    opacity: 0.6;
    font-size: 0.9rem;
  }
  form {
    max-width: 40rem;
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    font-size: 0.85rem;
  }
  .field > span {
    opacity: 0.75;
  }
  .field em {
    font-style: normal;
    opacity: 0.5;
    margin-left: 0.25rem;
  }
  textarea {
    font: inherit;
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 0.85rem;
    padding: 0.6rem 0.75rem;
    border-radius: 6px;
    border: 1px solid color-mix(in oklab, currentColor 18%, transparent);
    background: color-mix(in oklab, Canvas 100%, currentColor 2%);
    color: inherit;
    resize: vertical;
    min-height: 2.5rem;
  }
  .row {
    display: flex;
    gap: 0.6rem;
    margin-top: 0.25rem;
  }
  button {
    font: inherit;
    padding: 0.55rem 1rem;
    border-radius: 6px;
    border: 1px solid color-mix(in oklab, currentColor 22%, transparent);
    background: color-mix(in oklab, currentColor 8%, transparent);
    color: inherit;
    cursor: pointer;
  }
  button:hover:not(:disabled) {
    background: color-mix(in oklab, currentColor 14%, transparent);
  }
  button:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
  button.ghost {
    background: transparent;
  }
  .msg {
    margin: 0;
    font-size: 0.85rem;
  }
  .msg.err {
    color: color-mix(in oklab, currentColor 70%, crimson);
  }
  .msg.ok {
    color: color-mix(in oklab, currentColor 60%, seagreen);
  }
  .hint {
    font-size: 0.8rem;
  }
  .hint.warn {
    color: color-mix(in oklab, currentColor 70%, crimson);
  }
  code {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 0.85em;
  }
</style>
