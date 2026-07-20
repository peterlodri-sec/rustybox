# One-time account-side setup

Everything below is wired in the repo; these are the clicks/DNS only you can do.

## 1. GitHub Pages → rustybox.io

The `Pages` workflow (`.github/workflows/pages.yml`) already builds `site/` and
`site/CNAME` already contains `rustybox.io`.

1. **Repo → Settings → Pages → Build and deployment → Source: `GitHub Actions`.**
2. Push to `master` (or re-run the `Pages` workflow) so it deploys once.
3. Back on Settings → Pages, set **Custom domain = `rustybox.io`**, Save.
4. Tick **Enforce HTTPS** (available after the cert provisions, a few minutes).

### DNS at your registrar (for the apex `rustybox.io`)

Add GitHub Pages' apex records:

```
A     @   185.199.108.153
A     @   185.199.109.153
A     @   185.199.110.153
A     @   185.199.111.153
AAAA  @   2606:50c0:8000::153
AAAA  @   2606:50c0:8001::153
AAAA  @   2606:50c0:8002::153
AAAA  @   2606:50c0:8003::153
CNAME www peterlodri-sec.github.io.
```

Verify: `dig +short rustybox.io` returns the four A records; then the custom-domain
check in Settings → Pages goes green.

## 2. GitHub Sponsors profile

The Sponsor button (`.github/FUNDING.yml`) and the CTAs are live. To fill the
profile, open <https://github.com/sponsors/peterlodri-sec/dashboard> and paste
from [`sponsors-profile.md`](sponsors-profile.md):

1. **Profile → Introduction** ← the "Short bio".
2. **Featured work** ← the rustybox links.
3. **Goals** ← the `$500/mo → one dedicated day a week` goal.
4. **Tiers** → create four monthly tiers (🌱 $5 / 🔧 $15 / 🚀 $50 / 🛰️ $250)
   with the given titles + descriptions; add one-time $5/$25/$100.
5. **Welcome message** ← the auto-sent thank-you.

Keep the tier perks in sync with `README.md` and `site/index.html` if you edit them.
