<script lang="ts">
  // Add-contact form. The user's own y7 URI lives at the bottom as a
  // KeyDisplay block — this is the SINGLE canonical place where the local
  // identity is presented for copy/share. Removing the duplicate that used
  // to live in the sidebar was a deliberate brief point.

  import { sendContactRequestAction } from "../lib/stores/requests.svelte";
  import { identity } from "../lib/stores/identity.svelte";
  import { isValidY7Uri } from "../lib/format";
  import { openEmpty } from "../lib/stores/route.svelte";
  import Button from "../lib/components/Button.svelte";
  import Card from "../lib/components/Card.svelte";
  import Input from "../lib/components/Input.svelte";
  import Textarea from "../lib/components/Textarea.svelte";
  import KeyDisplay from "../lib/components/KeyDisplay.svelte";
  import { toast } from "../lib/components/toast.svelte";

  let y7Input = $state("");
  let greeting = $state("");
  let submitting = $state(false);

  const trimmed = $derived(y7Input.trim());
  const valid = $derived(isValidY7Uri(trimmed));
  const showInvalid = $derived(trimmed.length > 0 && !valid);

  async function submit(): Promise<void> {
    if (!valid || submitting) return;
    submitting = true;
    try {
      const g = greeting.trim();
      await sendContactRequestAction(trimmed, g.length === 0 ? null : g);
      toast.success("request sent");
      y7Input = "";
      greeting = "";
    } catch (err) {
      toast.error(err instanceof Error ? err.message : String(err));
    } finally {
      submitting = false;
    }
  }

  function cancel(): void {
    openEmpty();
  }
</script>

<section class="page">
  <div class="content">
    <header class="head">
      <h1>add contact</h1>
      <p class="sub">paste a y7 identity to send a contact request.</p>
    </header>

    <Card title="new request">
      <form
        class="form"
        onsubmit={(ev) => {
          ev.preventDefault();
          void submit();
        }}
      >
        <label class="field">
          <span class="label">identity</span>
          <Input
            bind:value={y7Input}
            placeholder="y7:…"
            ariaLabel="contact identity"
            invalid={showInvalid}
          />
          {#if showInvalid}
            <span class="hint warn">
              that doesn't look like a valid y7: identity.
            </span>
          {/if}
        </label>

        <label class="field">
          <span class="label">greeting <em>(optional)</em></span>
          <Textarea
            bind:value={greeting}
            placeholder="say hi"
            rows={3}
            ariaLabel="greeting"
          />
        </label>

        <div class="row">
          <Button
            type="submit"
            variant="primary"
            disabled={!valid || submitting}
            title="send contact request"
          >
            {submitting ? "sending…" : "send request"}
          </Button>
          <Button
            variant="ghost"
            disabled={submitting}
            onclick={cancel}
            title="cancel and return to chat list"
          >
            cancel
          </Button>
        </div>
      </form>
    </Card>

    {#if identity.y7Id !== null}
      <div class="my-key">
        <KeyDisplay
          value={identity.y7Id}
          label="your identity"
          layout="block"
        />
        <p class="hint">
          share this with the person you want to talk to. your private key
          never leaves this device.
        </p>
      </div>
    {/if}
  </div>
</section>

<style>
  .page {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: var(--y7-sp-8) var(--y7-sp-6);
    background: var(--y7-bg-base);
  }
  .content {
    max-width: 560px;
    margin: 0 auto;
    display: flex;
    flex-direction: column;
    gap: var(--y7-sp-6);
  }
  .head {
    display: flex;
    flex-direction: column;
    gap: var(--y7-sp-1);
  }
  h1 {
    margin: 0;
    font-size: var(--y7-fs-2xl);
    font-weight: var(--y7-fw-bold);
    color: var(--y7-text-primary);
    text-transform: lowercase;
    letter-spacing: 0.02em;
  }
  .sub {
    margin: 0;
    font-size: var(--y7-fs-md);
    color: var(--y7-text-secondary);
    text-transform: lowercase;
  }

  .form {
    display: flex;
    flex-direction: column;
    gap: var(--y7-sp-4);
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: var(--y7-sp-2);
  }
  .label {
    font-size: var(--y7-fs-sm);
    color: var(--y7-text-secondary);
    text-transform: lowercase;
    letter-spacing: 0.02em;
  }
  .label em {
    font-style: normal;
    color: var(--y7-text-muted);
    margin-left: var(--y7-sp-1);
  }
  .row {
    display: flex;
    gap: var(--y7-sp-2);
    margin-top: var(--y7-sp-1);
  }
  .hint {
    font-size: var(--y7-fs-sm);
    color: var(--y7-text-muted);
    text-transform: lowercase;
    margin: 0;
    line-height: var(--y7-lh-relaxed);
  }
  .hint.warn {
    color: var(--y7-red);
  }

  .my-key {
    display: flex;
    flex-direction: column;
    gap: var(--y7-sp-2);
    padding-top: var(--y7-sp-4);
    border-top: 1px solid var(--y7-border-subtle);
  }
</style>
