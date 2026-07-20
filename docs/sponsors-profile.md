# GitHub Sponsors — profile copy

The account's [Sponsors listing](https://github.com/sponsors/peterlodri-sec/dashboard)
already exists and funds crabcc/Vaked/the λ-Normalization Census — it's one
listing per account, not per-repo, so rustybox is added to it rather than
getting a separate one. GitHub's API has no mutation to edit an existing
listing's bio or an existing tier's description (only `createSponsorsTier`/
`publishSponsorsTier`/`retireSponsorsTier` — no update), so this stays a
manual paste. **This file is the source of truth**; keep it in sync with
`SPONSORS.md` / this README by hand whenever a tier description changes.

---

## Bio — add this bullet to "What you're funding"

Add alongside the existing crabcc / Vaked / λ-Normalization Census bullets:

> - 🦀 **[rustybox](https://github.com/peterlodri-sec/rustybox)** — BusyBox,
>   reborn in Rust: one fully-static, dual-architecture binary, migrating the
>   classic Unix toolbox onto memory-safe backends one applet at a time.
>   GPLv2 full edition + an MIT `rustybox-core`. [rustybox.io](https://rustybox.io)

## Tier descriptions (paste over the existing text for each tier)

- **$2 one-time — ☕ Tip**
  `| **one-time** | ☕ Tip | Buy a compute hour — thank-you note; credited in rustybox's SPONSORS.md one-time section |`

- **$5/mo — 🌱 Supporter**
  `| **$5/mo** | 🌱 Supporter | Name in BACKERS.md; sponsors-only dev-log; name in rustybox/SPONSORS.md |`

- **$25/mo — 🔧 Contributor**
  `| **$25/mo** | 🔧 Contributor | Submit 1 research experiment per month — I run it on the cluster and publish the open data + write-up, crediting you. + priority support; name in rustybox/SPONSORS.md |`

- **$100/mo — 🏗️ Backer**
  `| **$100/mo** | 🏗️ Backer | + your name/handle in the crabcc, Vaked & rustybox READMEs; early access to new datasets/releases |`

- **$500/mo — 🚀 Sponsor**
  `| **$500/mo** | 🚀 Sponsor | + 1 hr/mo office hours; direction input on crabcc/Vaked; your logo on rustybox.io |`

- **$2,500/mo — 🤝 Partner** *(orgs)*
  `| **$2,500/mo** | 🤝 Partner | + logo on crabcc + Vaked + rustybox.io; quarterly call; credited in releases; a say in which rustybox applets go memory-safe next |`

## What's automated vs. manual

- **Automated** (`.github/workflows/sponsors.yml`, triggered by the
  `sponsorship` webhook event): crediting a sponsor's GitHub handle in
  `SPONSORS.md`'s matching tier section, and in this README's Backer+ list,
  on `created`/`tier_changed`, and removing it on `cancelled`. Runs entirely
  within GitHub Actions — no external webhook receiver needed.
- **Manual, one-time**: the bio bullet and tier description text above (no
  API to push it); logo files for $500+ sponsors (they don't have one until
  the sponsor sends it — open an issue or ask in the welcome-message reply,
  then add it to `site/` and wire it into the rustybox.io template by hand).
- **Manual, ongoing**: honoring a sponsor's request for a different display
  name than their GitHub handle, or a request to be excluded even though
  their sponsorship is public — the automation always uses the GitHub login
  and always credits public sponsorships; treat a reply asking otherwise as
  an override and hand-edit `SPONSORS.md`/README after that point (the
  automation will leave hand edits alone until that sponsor's tier changes
  again, at which point re-apply the override).

## Welcome message (auto-sent to new sponsors)

> Thank you — genuinely. You're funding open, unglamorous systems work:
> crabcc, Vaked, and rustybox (BusyBox, made memory-safe in Rust). You're
> credited automatically under your GitHub handle in each project's
> SPONSORS.md / README as your tier allows — reply here with a different
> name/handle you'd like used instead, a logo file if you're $500+, or "keep
> me anonymous" to opt out entirely.
