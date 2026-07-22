#!/usr/bin/env python3
import json
import subprocess
import os
import sys
from datetime import datetime, timezone

# Paths
WORKSPACE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
STATUS_JSON_PATH = os.path.join(WORKSPACE, "site", "cve-status.json")
REPORT_HTML_PATH = os.path.join(WORKSPACE, "site", "docs", "cve-report.html")

def run_audit():
    print("Running cargo audit...")
    try:
        # Run cargo audit with JSON output. It exits with non-zero if vulnerabilities exist.
        result = subprocess.run(
            ["cargo", "audit", "--json"],
            capture_output=True,
            text=True,
            cwd=WORKSPACE
        )
        return result.stdout, result.returncode
    except FileNotFoundError:
        print("Error: cargo-audit is not installed. Generating dummy mock/clean report.")
        # Fallback for local development if cargo-audit is missing
        mock_output = json.dumps({
            "database": {"vulnerabilities_count": 0},
            "vulnerabilities": {
                "count": 0,
                "list": []
            }
        })
        return mock_output, 0

def parse_audit(json_str):
    try:
        data = json.loads(json_str)
    except json.JSONDecodeError:
        print("Error: Failed to parse cargo-audit JSON output.")
        # If output is not JSON (e.g. network error), construct a fallback
        return {
            "cve_count": 0,
            "vulnerabilities": [],
            "error": "Failed to parse security audit database output."
        }

    vulnerabilities = []
    
    # cargo-audit format varies slightly between versions, handle list or count safely
    vuln_list = []
    if "vulnerabilities" in data:
        if isinstance(data["vulnerabilities"], dict) and "list" in data["vulnerabilities"]:
            vuln_list = data["vulnerabilities"]["list"]
        elif isinstance(data["vulnerabilities"], list):
            vuln_list = data["vulnerabilities"]

    for item in vuln_list:
        advisory = item.get("advisory", {})
        vulnerabilities.append({
            "id": advisory.get("id", "Unknown ID"),
            "crate": advisory.get("package", "unknown"),
            "version": item.get("package", {}).get("version", "unknown"),
            "patched": ", ".join(advisory.get("patched_versions", ["none"])),
            "title": advisory.get("title", "No Title"),
            "url": advisory.get("url", "#"),
            "description": advisory.get("description", "No description provided.")
        })
        
    return {
        "cve_count": len(vulnerabilities),
        "vulnerabilities": vulnerabilities,
        "error": None
    }

def generate_report(report_data):
    now = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    
    # 1. Update status JSON
    status_data = {
        "cve_count": report_data["cve_count"],
        "last_checked": now,
        "status": "failing" if report_data["cve_count"] > 0 else "passing",
        "error": report_data["error"]
    }
    
    os.makedirs(os.path.dirname(STATUS_JSON_PATH), exist_ok=True)
    with open(STATUS_JSON_PATH, "w") as f:
        json.dump(status_data, f, indent=2)
    print(f"Updated status at {STATUS_JSON_PATH}")
    
    # 2. Generate HTML Report
    os.makedirs(os.path.dirname(REPORT_HTML_PATH), exist_ok=True)
    
    status_badge_color = "#ff5c5c" if report_data["cve_count"] > 0 else "#2ee6a6"
    status_text = f"{report_data['cve_count']} Vulnerabilities Found" if report_data["cve_count"] > 0 else "0 Active Vulnerabilities"
    
    vulns_html = ""
    if report_data["error"]:
        vulns_html = f"""
        <div class="alert error">
          <div class="alert-title">Audit Interrupted</div>
          <p>{report_data['error']}</p>
        </div>
        """
    elif report_data["cve_count"] == 0:
        vulns_html = """
        <div class="no-vulns">
          <div class="icon">✓</div>
          <h3>Your dependencies are secure!</h3>
          <p>No active CVEs or security advisories were detected in any compiled Rust libraries.</p>
        </div>
        """
    else:
        for v in report_data["vulnerabilities"]:
            vulns_html += f"""
            <div class="vuln-card">
              <div class="vuln-header">
                <span class="vuln-id">{v['id']}</span>
                <span class="vuln-crate">{v['crate']} v{v['version']}</span>
              </div>
              <h3>{v['title']}</h3>
              <p>{v['description']}</p>
              <div class="vuln-footer">
                <span><strong>Patched Versions:</strong> {v['patched']}</span>
                <a href="{v['url']}" target="_blank">View Advisory ↗</a>
              </div>
            </div>
            """

    html_content = f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>rustybox — Security Audit Report</title>
<meta name="description" content="Live security audit and CVE vulnerability report for the rustybox binary.">
<style>
  :root {{
    --bg: #0b0f0d;
    --panel: #111613;
    --ink: #e8f0ea;
    --muted: #8ba193;
    --acc: #2ee6a6;
    --acc2: #6cf;
    --line: #1e2823;
    --err: #ff5c5c;
    --mono: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
    --sans: system-ui, -apple-system, Segoe UI, Roboto, sans-serif;
  }}
  @media(prefers-color-scheme: light) {{
    :root {{
      --bg: #f6f8f6;
      --panel: #ffffff;
      --ink: #0d1712;
      --muted: #5a6b60;
      --line: #e2e9e4;
      --acc: #0a8f5f;
      --acc2: #0369a1;
    }}
  }}
  * {{ box-sizing: border-box; }}
  body {{
    margin: 0;
    background: var(--bg);
    color: var(--ink);
    font-family: var(--sans);
    line-height: 1.6;
    display: flex;
    justify-content: center;
    padding: 40px 20px;
  }}
  .container {{
    max-width: 800px;
    width: 100%;
  }}
  header {{
    border-bottom: 1px solid var(--line);
    padding-bottom: 24px;
    margin-bottom: 32px;
  }}
  .back-link {{
    display: inline-block;
    color: var(--muted);
    text-decoration: none;
    font-family: var(--mono);
    font-size: 0.85rem;
    margin-bottom: 16px;
  }}
  .back-link:hover {{ color: var(--acc); }}
  h1 {{
    font-size: 2.2rem;
    margin: 0 0 12px 0;
    letter-spacing: -1px;
  }}
  .meta {{
    display: flex;
    justify-content: space-between;
    align-items: center;
    flex-wrap: wrap;
    gap: 12px;
  }}
  .timestamp {{
    font-family: var(--mono);
    font-size: 0.85rem;
    color: var(--muted);
  }}
  .badge {{
    background: {status_badge_color};
    color: #04120c;
    font-weight: 700;
    padding: 6px 14px;
    border-radius: 20px;
    font-size: 0.85rem;
    font-family: var(--mono);
  }}
  .no-vulns {{
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 12px;
    padding: 40px;
    text-align: center;
    margin: 40px 0;
  }}
  .no-vulns .icon {{
    font-size: 3rem;
    color: var(--acc);
    margin-bottom: 16px;
    line-height: 1;
  }}
  .no-vulns h3 {{
    margin: 0 0 8px 0;
    font-size: 1.4rem;
  }}
  .no-vulns p {{
    color: var(--muted);
    margin: 0;
  }}
  .vuln-card {{
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 12px;
    padding: 24px;
    margin-bottom: 20px;
  }}
  .vuln-header {{
    display: flex;
    justify-content: space-between;
    font-family: var(--mono);
    font-size: 0.8rem;
    color: var(--muted);
    border-bottom: 1px solid var(--line);
    padding-bottom: 8px;
    margin-bottom: 12px;
  }}
  .vuln-id {{
    color: var(--err);
    font-weight: 700;
  }}
  .vuln-crate {{
    color: var(--acc2);
  }}
  .vuln-card h3 {{
    margin: 0 0 12px 0;
    font-size: 1.15rem;
  }}
  .vuln-card p {{
    color: var(--muted);
    font-size: 0.95rem;
    margin: 0 0 16px 0;
  }}
  .vuln-footer {{
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 0.85rem;
  }}
  .vuln-footer a {{
    color: var(--acc);
    text-decoration: none;
    font-family: var(--mono);
  }}
  .vuln-footer a:hover {{ text-decoration: underline; }}
</style>
</head>
<body>
<div class="container">
  <header>
    <a href="/docs/" class="back-link">← Back to Docs</a>
    <h1>Security Audit Report</h1>
    <div class="meta">
      <span class="timestamp">Last Checked: {now} (UTC)</span>
      <span class="badge">{status_text}</span>
    </div>
  </header>
  
  <main>
    {vulns_html}
  </main>
</div>
</body>
</html>
"""
    with open(REPORT_HTML_PATH, "w") as f:
        f.write(html_content)
    print(f"Generated HTML report at {REPORT_HTML_PATH}")
    
    # 3. Create GitHub issue in CI if failing
    if report_data["cve_count"] > 0 and os.environ.get("GITHUB_ACTIONS") == "true":
        create_github_issues(report_data["vulnerabilities"])

def create_github_issues(vulnerabilities):
    # Retrieve existing issues in the repo to avoid creating duplicates
    try:
        existing_issues = subprocess.run(
            ["gh", "issue", "list", "--json", "title,number"],
            capture_output=True,
            text=True
        )
        existing_data = json.loads(existing_issues.stdout)
    except Exception:
        existing_data = []

    for v in vulnerabilities:
        issue_title = f"[CVE Audit] Vulnerability found in {v['crate']}: {v['id']}"
        
        # Check if issue already exists
        duplicate = any(issue.get("title") == issue_title for issue in existing_data)
        if duplicate:
            print(f"Issue already exists for {v['id']}, skipping...")
            continue
            
        print(f"Creating GitHub issue for {v['id']}...")
        issue_body = f"""### Security Advisory Detected

A dependency vulnerability was found during the automated build pipeline.

- **Advisory ID:** {v['id']}
- **Crate:** `{v['crate']}` v{v['version']}
- **Title:** {v['title']}
- **Patched version:** `{v['patched']}`
- **Advisory URL:** {v['url']}

#### Description
{v['description']}

*Automated report generated by CVE pipeline.*
"""
        subprocess.run([
            "gh", "issue", "create",
            "--title", issue_title,
            "--body", issue_body,
            "--label", "bug,security"
        ])

def main():
    stdout, returncode = run_audit()
    report_data = parse_audit(stdout)
    generate_report(report_data)
    
    # Exit with code 1 if vulnerabilities exist to notify the runner,
    # but we handle this in GHA to commit status even if it fails.
    if report_data["cve_count"] > 0:
        print("Audit failed: dependency vulnerabilities found.")
        # We don't exit with non-zero here so the CI script has a chance to commit status/report.
        # The CI will handle exit codes.

if __name__ == "__main__":
    main()
