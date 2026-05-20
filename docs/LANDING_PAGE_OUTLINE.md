# Landing page outline — placeholder

Phase 17 polish. This document is the brief for the eventual
`aura.example` landing page. Nothing here is built — it's a record of
what the page needs to do.

## Audience

1. **The technically-curious knowledge-worker** who already uses
   Obsidian / Notion / Roam and is considering switching.
2. **The privacy-first writer** who's nervous about cloud LLM
   integration but wants the productivity.
3. **The research-tools-watcher** who finds Aura via a HN / arxiv link
   and wants to understand the cognitive-core claim before installing.

## Above-the-fold message

> Aura is a knowledge engine that thinks while you sleep.
> Local-first. Plain Markdown. Real cortex.

One-line value prop, one CTA ("Download for macOS / Windows / Linux"),
one screenshot — the cortex monitor running on a real vault.

## Six story sections (one screen each)

1. **Your notes, your machine.** Plain `.md` on disk. SQLite + LanceDB
   indices in a hidden `.aura/` folder. Uninstall Aura, your data is
   still there, openable in any editor.
2. **Wiki-links + transclusion that actually work.** Block-level UUIDs,
   `^anchor` markers, auto-healing on rename.
3. **Real semantic search, not lexical.** ONNX MiniLM / multilingual
   E5. BM25 + vector fusion via Hybrid RRF. Show a query that ranks
   conceptually-related notes above keyword matches.
4. **GraphRAG: the long answer.** Leiden 4-level community hierarchy,
   Haiku-summarised, Sonnet-answered. Show a "what patterns appear in
   my notes?" query returning cited communities.
5. **The cognitive core.** This is the line nobody else has.
   - Continuous attractor + Liquid State Machine + Langevin noise
     running in the background.
   - Free energy + curiosity drive reflections that appear in the
     Reflection Feed each morning.
   - Hopfield-style content-addressed retrieval.
   - Optional Hamiltonian phase-space fusion under telemetry.
6. **Open architecture.** MCP server with 13 tools. WebSocket Control
   Port for agents that subscribe to cortex events. Anthropic Skills
   for one-click workflows.

## Trust strip (footer-of-fold)

- "Hard rules" link → public commitment doc: no unwrap, no mock data,
  no key in logs, local-first absolute.
- Stand-in registry link → the public 🟢/🟡/🔴 table from
  [`STAND_IN_REGISTRY.md`](./STAND_IN_REGISTRY.md). Crucial — most
  knowledge tools overpromise; the registry is the differentiator.
- GitHub link (public after Phase 17 ships).
- Privacy statement: "your vault never leaves your device by default;
  Anthropic calls happen only with explicit per-operation consent and
  are listed in the audit-log panel."

## Download flow

- OS-detect the visitor; show their flavour first.
- Three builds: `.dmg` (signed + notarised), `.msi` (EV-signed),
  `.deb` + `.AppImage` + `.rpm` (GPG-signed). See
  [`CODE_SIGNING_GUIDE.md`](./CODE_SIGNING_GUIDE.md).
- Show the SHA-256 next to each link so the security-minded can verify.

## Page implementation notes

- Static. Plain HTML + a single CSS file. No JS framework needed.
  ~50 KB total page weight target.
- Two languages on day one: English and Arabic (mirror Aura's vault
  test fixture). RTL flip is a single CSS variable.
- Hosted from the same bucket as the auto-update endpoint so the
  TLS cert + DNS are one config.

## Not on the page (deliberately)

- No buzzwords. No "AI-powered." No "transform your workflow." If the
  cortex screenshot doesn't sell it, the marketing copy won't.
- No newsletter signup. No tracking. No cookies. No "we use cookies"
  banner because there isn't one.

## Tracker (Phase 17 sub-items)

- [ ] Static site scaffold under `apps/landing/`
- [ ] Cortex monitor live-render screenshot for hero
- [ ] EN + AR copy review
- [ ] Hosting + DNS (bucket + TLS)
- [ ] First-release download links + SHA-256 footer
