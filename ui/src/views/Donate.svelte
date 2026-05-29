<script lang="ts">
  // Static donate page — crypto addresses only, no backend. Each row copies
  // its address to the clipboard. Same page shell as the other views.

  import Card from "../lib/components/Card.svelte";
  import IconButton from "../lib/components/IconButton.svelte";
  import { toast } from "../lib/components/toast.svelte";

  interface Wallet {
    ticker: string;
    name: string;
    address: string;
  }

  // Addresses are static.
  const WALLETS: Wallet[] = [
    { ticker: "BTC", name: "bitcoin", address: "bc1qfmnvkt2aj9k8jnaf5s7snr3gl2mmdz9m6ug2du" },
    { ticker: "ETH", name: "ethereum", address: "0x1A13BF0847cbb2c0699ef61F10Bc1beb995ac492" },
    { ticker: "LTC", name: "litecoin", address: "Lh3PQZTcSxbDxPVTN6AgAQx3xYWwsbcWmm" },
    { ticker: "SOL", name: "solana", address: "8fzQ6a3xpvZGAUWxphSdLb53NgGMn3e2nSqcwG1K1cDT" },
  ];

  async function copyAddr(w: Wallet): Promise<void> {
    try {
      await navigator.clipboard.writeText(w.address);
      toast.success(`${w.ticker} address copied`);
    } catch {
      toast.error("copy failed");
    }
  }
</script>

{#snippet copyGlyph()}
  <svg
    viewBox="0 0 24 24"
    width="15"
    height="15"
    fill="none"
    stroke="currentColor"
    stroke-width="1.8"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
  >
    <rect x="9" y="9" width="11" height="11" rx="2" />
    <path d="M5 15 H4 a1 1 0 0 1 -1 -1 V4 a1 1 0 0 1 1 -1 h10 a1 1 0 0 1 1 1 v1" />
  </svg>
{/snippet}

<section class="page">
  <div class="content">
    <header class="head">
      <div class="title-row">
        <!-- pick-me line-art kitty -->
        <svg
          class="cat"
          viewBox="0 0 32 32"
          width="30"
          height="30"
          fill="none"
          stroke="currentColor"
          stroke-width="1.6"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        >
          <path d="M9 10 L7 4 L13.5 7.5" />
          <path d="M23 10 L25 4 L18.5 7.5" />
          <ellipse cx="16" cy="16" rx="9.5" ry="8.5" />
          <path d="M11.5 15.5 l1.6 -1.6 l1.6 1.6" />
          <path d="M17.3 15.5 l1.6 -1.6 l1.6 1.6" />
          <path d="M14.8 19 l1.2 1 l1.2 -1" />
          <path d="M5.5 16 h3.5 M5.5 19 h3.5" />
          <path d="M26.5 16 h-3.5 M26.5 19 h-3.5" />
        </svg>
        <h1>donate</h1>
      </div>
      <p class="sub">support project — every bit keeps y7ke independent &gt;//&lt;</p>
    </header>

    <Card title="wallets">
      <ul class="wallets">
        {#each WALLETS as w (w.ticker)}
          <li class="wallet">
            <span class="ticker" title={w.name}>{w.ticker}</span>
            <code class="addr">{w.address}</code>
            <IconButton
              ariaLabel="copy {w.ticker} address"
              title="copy {w.ticker} address"
              onclick={() => copyAddr(w)}
            >
              {@render copyGlyph()}
            </IconButton>
          </li>
        {/each}
      </ul>
    </Card>

    <p class="note">
      no accounts, no fees, no middleman — addresses are static and copy
      straight to your clipboard. thank you ♡
    </p>
  </div>
</section>

<style>
  .page {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: var(--y7-sp-6) var(--y7-sp-6);
    background: var(--y7-bg-base);
  }
  .content {
    max-width: 640px;
    width: 100%;
    margin: 0 auto;
    display: flex;
    flex-direction: column;
    gap: var(--y7-sp-5);
  }
  .head {
    display: flex;
    flex-direction: column;
    gap: var(--y7-sp-1);
  }
  .title-row {
    display: flex;
    align-items: center;
    gap: var(--y7-sp-2);
  }
  .cat {
    color: var(--y7-text-secondary);
    flex-shrink: 0;
  }
  h1 {
    margin: 0;
    /* a hair bigger than the other pages' headings (fs-2xl) — per brief. */
    font-size: var(--y7-fs-3xl);
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

  .wallets {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--y7-sp-2);
  }
  .wallet {
    display: flex;
    align-items: center;
    gap: var(--y7-sp-3);
    padding: var(--y7-sp-2) var(--y7-sp-3);
    background: var(--y7-bg-base);
    border: 1px solid var(--y7-border-subtle);
    border-radius: var(--y7-r-md);
  }
  .ticker {
    flex-shrink: 0;
    width: 34px;
    font-size: var(--y7-fs-sm);
    font-weight: var(--y7-fw-semibold);
    color: var(--y7-text-secondary);
    letter-spacing: 0.04em;
  }
  .addr {
    flex: 1 1 auto;
    min-width: 0;
    /* break-all so the full address always shows + wraps at any width
     * (responsive) instead of overflowing or being truncated. */
    word-break: break-all;
    font-family: var(--y7-font-mono);
    font-size: var(--y7-fs-sm);
    color: var(--y7-text-primary);
    line-height: var(--y7-lh-normal);
  }

  .note {
    margin: 0;
    font-size: var(--y7-fs-sm);
    color: var(--y7-text-muted);
    text-transform: lowercase;
    line-height: var(--y7-lh-relaxed);
  }
</style>
