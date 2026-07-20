#!/usr/bin/env python3
"""Sync SPONSORS.md / README.md against a GitHub `sponsorship` webhook event.

Invoked by .github/workflows/sponsors.yml with $GITHUB_EVENT_PATH pointing at
the event JSON. Reads the sponsor's tier price, maps it onto the matching
`<!-- sponsors:KEY:start -->...<!-- sponsors:KEY:end -->` marker block in
SPONSORS.md (and, for Backer-and-up tiers, the equivalent block in
README.md), and adds/removes/moves that sponsor's line. Idempotent: safe to
re-run for the same event, and a `tier_changed` first removes the sponsor
from every bucket before re-adding to the new one, so a move never leaves a
stale duplicate entry behind.

Respects `privacy_level: "private"` by never listing that sponsor anywhere.
"""
import json
import os
import re
import sys

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SPONSORS_MD = os.path.join(REPO_ROOT, "SPONSORS.md")
README_MD = os.path.join(REPO_ROOT, "README.md")

# GitHub tier monthly price (dollars) -> SPONSORS.md marker key. One-time
# tiers are bucketed separately regardless of amount.
BUCKET_KEYS = ["2500", "500", "100", "25", "5"]
README_BUCKETS = {"2500", "500", "100"}  # Backer ($100) and up get README credit


def bucket_for(tier):
    if tier.get("is_one_time"):
        return "onetime"
    dollars = tier.get("monthly_price_in_dollars")
    if dollars is None:
        cents = tier.get("monthly_price_in_cents", 0)
        dollars = cents // 100
    key = str(int(dollars))
    return key if key in BUCKET_KEYS or key == "onetime" else None


def sponsor_line(login):
    return f"- [@{login}](https://github.com/{login})"


def edit_marker_block(content, key, mutate):
    """mutate(lines) -> new_lines, applied to the lines between the
    sponsors:{key}:start/end markers. Restores "_Be the first._" if the
    block ends up empty. No-op (returns content unchanged) if the markers
    aren't found."""
    start_marker = f"<!-- sponsors:{key}:start -->"
    end_marker = f"<!-- sponsors:{key}:end -->"
    start = content.find(start_marker)
    end = content.find(end_marker)
    if start == -1 or end == -1 or end < start:
        return content
    inner_start = start + len(start_marker)
    inner = content[inner_start:end]
    lines = [l for l in inner.strip("\n").split("\n") if l.strip() and l.strip() != "_Be the first._"]
    lines = mutate(lines)
    if not lines:
        lines = ["_Be the first._"]
    new_inner = "\n" + "\n".join(lines) + "\n"
    return content[:inner_start] + new_inner + content[end:]


def add_to_bucket(content, key, login):
    line = sponsor_line(login)

    def mutate(lines):
        if line in lines:
            return lines
        return lines + [line]

    return edit_marker_block(content, key, mutate)


def sync_file(path, login, bucket, action, in_scope_buckets, marker_for=lambda b: b):
    """`in_scope_buckets` are the price buckets this file cares about at all;
    `marker_for` maps a bucket to the marker key actually used in this file
    (SPONSORS.md's markers are one-per-bucket, so identity; README's is one
    combined `backers` marker for every bucket in README_BUCKETS)."""
    if not os.path.exists(path):
        return
    with open(path, encoding="utf-8") as f:
        content = f.read()

    original = content
    marker_keys = {marker_for(b) for b in in_scope_buckets}
    content = remove_from_markers(content, login, marker_keys)
    if action in ("created", "tier_changed", "edited") and bucket in in_scope_buckets:
        content = add_to_bucket(content, marker_for(bucket), login)

    if content != original:
        with open(path, "w", encoding="utf-8") as f:
            f.write(content)


def remove_from_markers(content, login, marker_keys):
    line = sponsor_line(login)
    for key in marker_keys:
        content = edit_marker_block(content, key, lambda ls, line=line: [l for l in ls if l != line])
    return content


def main():
    event_path = os.environ.get("GITHUB_EVENT_PATH")
    if not event_path:
        print("GITHUB_EVENT_PATH not set", file=sys.stderr)
        return 1
    with open(event_path, encoding="utf-8") as f:
        event = json.load(f)

    sponsorship = event.get("sponsorship", {})
    action = event.get("action", "")
    sponsor = sponsorship.get("sponsor", {})
    login = sponsor.get("login")
    privacy = sponsorship.get("privacy_level", "public")
    tier = sponsorship.get("tier", {})

    if not login:
        print("No sponsor login in event payload, nothing to do.")
        return 0

    # Advance notices, not final state changes -- nothing to credit/remove yet.
    if action in ("pending_cancellation", "pending_tier_change"):
        print(f"Ignoring advance-notice action '{action}' for @{login}.")
        return 0

    readme_marker = lambda _bucket: "backers"

    if privacy == "private":
        # Never list a private sponsor; if they were previously public and
        # switched to private, this also removes any stale credit.
        sync_file(SPONSORS_MD, login, None, "cancelled", BUCKET_KEYS + ["onetime"])
        sync_file(README_MD, login, None, "cancelled", README_BUCKETS, marker_for=readme_marker)
        print(f"@{login} is a private sponsor -- removed/kept out of public credits.")
        return 0

    bucket = bucket_for(tier) if action != "cancelled" else None

    sync_file(SPONSORS_MD, login, bucket, action, BUCKET_KEYS + ["onetime"])
    sync_file(README_MD, login, bucket, action, README_BUCKETS, marker_for=readme_marker)

    print(f"Synced @{login}: action={action} bucket={bucket}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
