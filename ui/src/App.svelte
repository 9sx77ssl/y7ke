<script lang="ts">
  // Minimal placeholder for M0. Real views (IdentitySetup, Contacts, AddContact,
  // Requests, Chat, Settings) land in M1 once Tauri commands are wired up.
  const status = "scaffold";
</script>

<main>
  <header>
    <h1>Y7KE</h1>
    <p class="tag">Privacy-first, key-based, peer-to-peer messenger.</p>
  </header>
  <section>
    <p>Status: <code>{status}</code></p>
    <p class="hint">M0 scaffold — V1 capabilities arrive in M1.</p>
  </section>
</main>

<style>
  :global(:root) {
    color-scheme: light dark;
  }
  main {
    max-width: 720px;
    margin: 0 auto;
    padding: 4rem 2rem;
    font-family: ui-sans-serif, system-ui, -apple-system, "Segoe UI", Roboto, sans-serif;
  }
  header {
    margin-bottom: 2rem;
  }
  h1 {
    margin: 0;
    font-size: 2rem;
    letter-spacing: -0.02em;
  }
  .tag {
    margin: 0.5rem 0 0;
    opacity: 0.7;
    font-size: 0.95rem;
  }
  section {
    border-top: 1px solid currentColor;
    border-color: color-mix(in oklab, currentColor 20%, transparent);
    padding-top: 1rem;
  }
  code {
    padding: 0.1rem 0.4rem;
    background: color-mix(in oklab, currentColor 8%, transparent);
    border-radius: 4px;
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 0.85rem;
  }
  .hint {
    opacity: 0.6;
    font-size: 0.85rem;
  }
</style>
